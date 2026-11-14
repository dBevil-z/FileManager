use super::{
    CHUNK, Listing, TIMEOUT, check_cancel, check_size, cleanup_message, name, parent,
    profile::{Config, Provider, Secret, SshAuth},
    progress, remote_file, source_unchanged,
};
use crate::{Core, Result, err, paths};
use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use ssh2::{
    CheckResult, ErrorCode, HashType, KnownHostFileKind, OpenFlags, OpenType, RenameFlags, Session,
    Sftp,
};
use std::{
    fs::{File, Metadata},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

pub(super) struct Client {
    sftp: Sftp,
    root: PathBuf,
}

fn message(e: ssh2::Error) -> String {
    match e.code() {
        ErrorCode::SFTP(2 | 10) => "SFTP 文件或目录不存在".into(),
        ErrorCode::SFTP(3) => "SFTP 服务拒绝访问".into(),
        ErrorCode::SFTP(11) => "远程已有同名项目，未覆盖".into(),
        ErrorCode::SFTP(18) => "只能删除空的远程文件夹".into(),
        code => format!("SFTP 请求失败（{code:?}），请检查路径、权限和服务器状态"),
    }
}

impl Client {
    pub fn reader(self, path: &str) -> Result<Reader> {
        let dir = self
            .sftp
            .opendir(&self.checked_dir(path)?)
            .map_err(message)?;
        Ok(Reader {
            dir,
            path: path.into(),
        })
    }

    pub fn ensure_dir(&mut self, path: &str) -> Result<()> {
        let child = self.child(path)?;
        match self.sftp.lstat(&child) {
            Ok(meta) if meta.is_dir() => Ok(()),
            Ok(_) => Err("远程已有同名项目，不能创建文件夹".into()),
            Err(e) if e.code() == ErrorCode::SFTP(2) => self.mkdir(path),
            Err(e) => Err(message(e)),
        }
    }
    pub async fn connect(core: &Core, config: &Config, secret: &Secret) -> Result<Self> {
        let Provider::Sftp {
            user,
            auth,
            key_file,
        } = &config.provider
        else {
            return Err("无效的 SFTP 连接配置".into());
        };
        let origin = config.origin()?;
        let host = origin
            .host_str()
            .ok_or("服务器地址无效")?
            .trim_matches(['[', ']']);
        let port = origin.port().unwrap_or(22);
        // Bound DNS and connect together; handshake and subsequent SSH calls use the session timeout.
        let socket = super::deadline(async {
            let socket = tokio::net::TcpStream::connect((host, port))
                .await
                .map_err(|_| "无法连接 SFTP 服务器，请检查地址和端口")?;
            let socket = socket.into_std().map_err(err)?;
            socket.set_nonblocking(false).map_err(err)?;
            Ok(socket)
        })
        .await?;
        let mut session = Session::new().map_err(message)?;
        session.set_timeout(TIMEOUT.as_millis() as u32);
        session.set_tcp_stream(socket);
        session.handshake().map_err(message)?;

        let (key, _) = session.host_key().ok_or("SFTP 服务器没有提供主机密钥")?;
        let mut known = session.known_hosts().map_err(message)?;
        for file in [
            PathBuf::from("/etc/ssh/ssh_known_hosts"),
            core.home_dir.join(".ssh/known_hosts"),
        ] {
            if file.exists() {
                known
                    .read_file(&file, KnownHostFileKind::OpenSSH)
                    .map_err(|_| "无法读取 SSH known_hosts 文件")?;
            }
        }
        let fingerprint = session
            .host_key_hash(HashType::Sha256)
            .map(|h| format!("SHA256:{}", STANDARD_NO_PAD.encode(h)))
            .unwrap_or_default();
        match known.check_port(host, port, key) {
            CheckResult::Match => (),
            CheckResult::Mismatch => {
                return Err(format!(
                    "SFTP 主机密钥已更改，请向服务器管理员核对 {fingerprint}"
                ));
            }
            _ => {
                return Err(format!(
                    "SFTP 主机尚未信任，请用 SSH 核对并加入 known_hosts：{fingerprint}"
                ));
            }
        }

        let auth_result = match auth {
            SshAuth::Password => session.userauth_password(user, &secret.value),
            SshAuth::Key => session.userauth_pubkey_file(
                user,
                None,
                &paths::decode(core, key_file)?,
                (!secret.value.is_empty()).then_some(secret.value.as_str()),
            ),
            SshAuth::Agent => session.userauth_agent(user),
        };
        if auth_result.is_err() || !session.authenticated() {
            return Err("SFTP 登录失败，请检查用户名、密码、私钥或 SSH Agent".into());
        }
        let sftp = session.sftp().map_err(message)?;
        let requested = if config.root.is_empty() {
            PathBuf::from(".")
        } else {
            Path::new("/").join(&config.root)
        };
        let root = sftp.realpath(&requested).map_err(message)?;
        if !root.is_absolute() || !sftp.stat(&root).map_err(message)?.is_dir() {
            return Err("SFTP 起始位置不是目录".into());
        }
        Ok(Self { sftp, root })
    }

    fn checked_dir(&self, path: &str) -> Result<PathBuf> {
        let expected = remote_file(&self.root, path);
        let real = self.sftp.realpath(&expected).map_err(message)?;
        if real != expected || !real.starts_with(&self.root) {
            return Err("不能通过符号链接访问远程目录".into());
        }
        Ok(real)
    }

    fn child(&self, path: &str) -> Result<PathBuf> {
        Ok(self.checked_dir(parent(path))?.join(name(path)))
    }

    pub fn list(&mut self, path: &str, limit: usize) -> Result<Listing> {
        let remote = self.checked_dir(path)?;
        let mut dir = self.sftp.opendir(&remote).map_err(message)?;
        let mut listing = Listing::default();
        let start = Instant::now();
        let mut seen = 0;
        loop {
            if start.elapsed() >= TIMEOUT {
                return Err("远程目录读取超时".into());
            }
            let (entry, stat) = match dir.readdir() {
                Ok(item) => item,
                Err(e) if e.message() == "no more files" => break,
                Err(e) => return Err(message(e)),
            };
            if entry == Path::new(".") || entry == Path::new("..") {
                continue;
            }
            if seen == limit {
                listing.truncated = true;
                break;
            }
            seen += 1;
            if let Some(name) = entry.to_str().filter(|_| stat.is_dir() || stat.is_file()) {
                listing.push(
                    path,
                    name,
                    stat.is_dir(),
                    stat.size,
                    stat.mtime
                        .and_then(|v| i64::try_from(v).ok()?.checked_mul(1000)),
                );
            } else {
                listing.skipped += 1;
            }
            if limit == 1 {
                break;
            }
        }
        Ok(listing)
    }

    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        self.sftp.mkdir(&self.child(path)?, 0o755).map_err(message)
    }

    pub fn delete(&mut self, path: &str, is_dir: bool) -> Result<()> {
        let path = self.child(path)?;
        let meta = self.sftp.lstat(&path).map_err(message)?;
        if is_dir && meta.is_dir() {
            self.sftp.rmdir(&path).map_err(message)
        } else if !is_dir && meta.is_file() {
            self.sftp.unlink(&path).map_err(message)
        } else {
            Err("远程项目的类型已更改，未删除".into())
        }
    }

    pub fn upload(&mut self, path: &str, source: &mut File, meta: &Metadata) -> Result<u64> {
        let dest = self.child(path)?;
        match self.sftp.lstat(&dest) {
            Ok(_) => return Err("远程已有同名项目，未覆盖".into()),
            Err(e) if e.code() == ErrorCode::SFTP(2) => (),
            Err(e) => return Err(message(e)),
        }
        let staging = dest.with_file_name(format!(".filemanager-upload-{}", uuid::Uuid::new_v4()));
        let mut target = self
            .sftp
            .open_mode(
                &staging,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
                0o600,
                OpenType::File,
            )
            .map_err(message)?;
        let result = (|| {
            let mut buf = vec![0; CHUNK];
            let mut sent = 0;
            while sent < meta.len() {
                check_cancel()?;
                let n = ((meta.len() - sent).min(CHUNK as u64)) as usize;
                source.read_exact(&mut buf[..n]).map_err(err)?;
                target.write_all(&buf[..n]).map_err(err)?;
                sent += n as u64;
                progress(path, sent, meta.len());
            }
            source_unchanged(source, meta, sent)?;
            target.close().map_err(message)?;
            check_cancel()?;
            self.sftp
                .rename(&staging, &dest, Some(RenameFlags::empty()))
                .map_err(message)?;
            Ok(sent)
        })();
        drop(target);
        result.map_err(|error| cleanup_message(error, self.sftp.unlink(&staging).map_err(message)))
    }

    pub fn download(&mut self, path: &str, target: &mut File) -> Result<u64> {
        let remote = self.child(path)?;
        let meta = self.sftp.lstat(&remote).map_err(message)?;
        if !meta.is_file() {
            return Err("只能下载普通文件".into());
        }
        let size = meta.size.ok_or("SFTP 服务没有提供文件大小")?;
        let mut file = self.sftp.open(&remote).map_err(message)?;
        let opened = file.stat().map_err(message)?;
        if opened.size != meta.size || opened.mtime != meta.mtime {
            return Err("远程文件已更改，请重试".into());
        }
        progress(path, 0, size);
        let mut buf = vec![0; CHUNK];
        let mut received = 0;
        loop {
            check_cancel()?;
            let n = file.read(&mut buf).map_err(err)?;
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > size {
                return Err("远程文件已更改，请重试".into());
            }
            target.write_all(&buf[..n]).map_err(err)?;
            progress(path, received, size);
        }
        check_size(received, size)?;
        if file.stat().map_err(message)?.mtime != meta.mtime {
            return Err("远程文件已更改，请重试".into());
        }
        file.close().map_err(message)?;
        Ok(received)
    }
}

pub(super) struct Reader {
    dir: ssh2::File,
    path: String,
}

impl Reader {
    pub fn next(&mut self) -> Result<Option<super::pagination::Step>> {
        loop {
            let (entry, stat) = match self.dir.readdir() {
                Ok(item) => item,
                Err(e) if e.message() == "no more files" => return Ok(None),
                Err(e) => return Err(message(e)),
            };
            if entry == Path::new(".") || entry == Path::new("..") {
                continue;
            }
            return Ok(Some(
                match entry.to_str().filter(|_| stat.is_dir() || stat.is_file()) {
                    Some(name) => super::pagination::entry(
                        &self.path,
                        name,
                        stat.is_dir(),
                        stat.size,
                        stat.mtime
                            .and_then(|v| i64::try_from(v).ok()?.checked_mul(1000)),
                    ),
                    None => super::pagination::Step::Skipped,
                },
            ));
        }
    }
}
