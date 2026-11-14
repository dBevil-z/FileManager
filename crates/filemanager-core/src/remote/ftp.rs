use super::{
    CHUNK, Listing, check_cancel, check_size, cleanup_message, deadline, name, parent,
    profile::{Config, Provider, Secret},
    progress, source_unchanged, tls_config,
};
use crate::{Result, err};
use std::{
    fs::{File, Metadata},
    io::{Read, Write},
    net::SocketAddr,
    sync::Arc,
};
use suppaftp::{
    FtpError, Status,
    list::ListParser,
    tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream},
    types::{FileType, Mode},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

type Stream = AsyncRustlsFtpStream;
const MAX_LIST_LINE: u64 = 16 * 1024;
const CONFLICT: &str = "远程已有同名项目，未覆盖";

pub(super) struct Client {
    stream: Stream,
    secure: bool,
}

fn message(e: FtpError) -> String {
    match e {
        FtpError::UnexpectedResponse(response) => match response.status {
            Status::InvalidCredentials | Status::NotLoggedIn => {
                "FTP 登录失败，请检查用户名和密码".into()
            }
            Status::FileUnavailable => "FTP 路径不存在、没有权限，或目录非空".into(),
            status => format!("FTP 服务器拒绝请求（{}）", status.code()),
        },
        FtpError::SecureError(_) => "FTPS 握手失败，请检查证书信任和显式 TLS 设置".into(),
        _ => "FTP 连接或数据传输失败，请检查服务器状态和被动模式端口".into(),
    }
}

fn unsupported(e: &FtpError) -> bool {
    matches!(e, FtpError::UnexpectedResponse(r)
        if matches!(r.status, Status::BadCommand | Status::NotImplemented | Status::NotImplementedParameter))
}

impl Client {
    pub async fn reader(mut self, path: &str) -> Result<Reader> {
        let suffix = if path.is_empty() {
            String::new()
        } else {
            format!(" {}", path.trim_end_matches('/'))
        };
        let codes = [Status::AboutToSend, Status::AlreadyOpen];
        let (mlsd, data) = deadline(async {
            match self
                .stream
                .custom_data_command(format!("MLSD{suffix}"), &codes)
                .await
            {
                Ok((_, data)) => Ok((true, data)),
                Err(e) if unsupported(&e) => self
                    .stream
                    .custom_data_command(format!("LIST{suffix}"), &codes)
                    .await
                    .map(|(_, data)| (false, data))
                    .map_err(message),
                Err(e) => Err(message(e)),
            }
        })
        .await?;
        Ok(Reader {
            client: self,
            data: Some(BufReader::new(data)),
            mlsd,
            path: path.into(),
        })
    }

    pub async fn ensure_dir(&mut self, path: &str) -> Result<()> {
        let listing = self.list(parent(path), super::MAX_ENTRIES).await?;
        if listing.truncated || listing.skipped > 0 {
            return Err("无法检查 FTP 目录中的全部文件名，请选择较小的目录".into());
        }
        let expected = name(path).to_lowercase();
        if let Some(entry) = listing
            .entries
            .iter()
            .find(|e| e.name.to_lowercase() == expected)
        {
            return if entry.is_dir && entry.name == name(path) {
                Ok(())
            } else {
                Err("远程已有同名项目，不能创建文件夹".into())
            };
        }
        self.mkdir(path).await
    }
    pub async fn connect(config: &Config, secret: &Secret) -> Result<Self> {
        let Provider::Ftp { user } = &config.provider else {
            return Err("无效的 FTP 连接配置".into());
        };
        let origin = config.origin()?;
        let host = origin
            .host_str()
            .ok_or("服务器地址无效")?
            .trim_matches(['[', ']']);
        let secure = origin.scheme() == "ftps";
        deadline(async {
            let mut stream = Stream::connect((host, origin.port().unwrap_or(21)))
                .await
                .map_err(message)?;
            if secure {
                stream = stream
                    .into_secure(
                        AsyncRustlsConnector::from(suppaftp::tokio_rustls::TlsConnector::from(
                            Arc::new(tls_config()?),
                        )),
                        host,
                    )
                    .await
                    .map_err(message)?;
            }
            let peer = stream.get_ref().peer_addr().map_err(err)?;
            if peer.is_ipv6() {
                stream.set_mode(Mode::ExtendedPassive);
            }
            // Only connect the passive data socket to the control peer; ignore PASV's supplied host.
            stream = stream.passive_stream_builder(move |addr| {
                Box::pin(async move {
                    tokio::net::TcpStream::connect(SocketAddr::new(peer.ip(), addr.port()))
                        .await
                        .map_err(FtpError::ConnectionError)
                })
            });
            stream.login(user, &secret.value).await.map_err(message)?;
            stream
                .transfer_type(FileType::Binary)
                .await
                .map_err(message)?;
            if !config.root.is_empty() {
                stream
                    .cwd(format!("/{}", config.root.trim_start_matches('/')))
                    .await
                    .map_err(message)?;
            }
            Ok(Self { stream, secure })
        })
        .await
    }

    pub async fn list(&mut self, dir: &str, limit: usize) -> Result<Listing> {
        deadline(async {
            let suffix = if dir.is_empty() {
                String::new()
            } else {
                format!(" {}", dir.trim_end_matches('/'))
            };
            let codes = [Status::AboutToSend, Status::AlreadyOpen];
            let (mlsd, data) = match self
                .stream
                .custom_data_command(format!("MLSD{suffix}"), &codes)
                .await
            {
                Ok((_, data)) => (true, data),
                Err(e) if unsupported(&e) => {
                    let (_, data) = self
                        .stream
                        .custom_data_command(format!("LIST{suffix}"), &codes)
                        .await
                        .map_err(message)?;
                    (false, data)
                }
                Err(e) => return Err(message(e)),
            };
            let mut data = BufReader::new(data);
            let mut result = Listing::default();
            let mut seen = 0;
            loop {
                let mut line = Vec::new();
                let count = (&mut data)
                    .take(MAX_LIST_LINE + 1)
                    .read_until(b'\n', &mut line)
                    .await
                    .map_err(err)?;
                if count == 0 {
                    break;
                }
                if count as u64 > MAX_LIST_LINE {
                    return Err("FTP 目录中存在过长的条目".into());
                }
                let Ok(line) = std::str::from_utf8(&line) else {
                    result.skipped += 1;
                    seen += 1;
                    if seen >= limit {
                        result.truncated = true;
                        return Ok(result);
                    }
                    continue;
                };
                let line = line.trim_end_matches(['\r', '\n']);
                if line.is_empty() || (!mlsd && line.starts_with("total ")) {
                    continue;
                }
                if seen == limit {
                    result.truncated = true;
                    return Ok(result);
                }
                seen += 1;
                let (parsed, original_name) = if mlsd {
                    // The list parser treats semicolons in a name as fact delimiters.
                    // Parse only the metadata and keep the name exactly as sent.
                    let Some((facts, name)) = line.split_once("; ") else {
                        result.skipped += 1;
                        continue;
                    };
                    (ListParser::parse_mlsd(&format!("{facts}; _")), Some(name))
                } else {
                    (line.parse(), None)
                };
                match parsed {
                    Ok(entry) => {
                        let name = original_name.unwrap_or_else(|| entry.name());
                        if matches!(name, "." | "..") {
                            seen -= 1;
                            continue;
                        }
                        if !(entry.is_directory() || entry.is_file()) {
                            result.skipped += 1;
                            continue;
                        }
                        let modified = chrono::DateTime::<chrono::Utc>::from(entry.modified())
                            .timestamp_millis();
                        result.push(
                            dir,
                            name,
                            entry.is_directory(),
                            Some(entry.size() as u64),
                            Some(modified),
                        );
                    }
                    _ => result.skipped += 1,
                }
                if limit == 1 && seen > 0 {
                    return Ok(result);
                }
            }
            self.stream
                .close_data_connection(data.into_inner())
                .await
                .map_err(message)?;
            Ok(result)
        })
        .await
    }

    pub async fn mkdir(&mut self, path: &str) -> Result<()> {
        deadline(async {
            self.stream
                .mkdir(path.trim_end_matches('/'))
                .await
                .map_err(message)
        })
        .await
    }

    pub async fn delete(&mut self, path: &str, is_dir: bool) -> Result<()> {
        deadline(async {
            if is_dir {
                self.stream
                    .rmdir(path.trim_end_matches('/'))
                    .await
                    .map_err(message)
            } else {
                self.stream.rm(path).await.map_err(message)
            }
        })
        .await
    }

    async fn target_free(&mut self, path: &str) -> Result<()> {
        // Let the server resolve the name first (including case-insensitive aliases).
        match deadline(async { Ok(self.stream.mlst(Some(path)).await) }).await? {
            Ok(_) => return Err(CONFLICT.into()),
            Err(FtpError::UnexpectedResponse(response))
                if response.status == Status::FileUnavailable =>
            {
                ()
            }
            Err(e) if unsupported(&e) => (),
            Err(e) => return Err(message(e)),
        }
        let listing = self.list(parent(path), super::MAX_ENTRIES).await?;
        if listing.truncated || listing.skipped > 0 {
            // Refuse an unsafe guess when an unparsed or unseen name could match.
            return Err("无法检查 FTP 目录中的全部文件名，请选择较小的目录".into());
        }
        let normalized = name(path).to_lowercase();
        if listing
            .entries
            .iter()
            .any(|e| e.name.to_lowercase() == normalized)
        {
            return Err(CONFLICT.into());
        }
        Ok(())
    }

    pub async fn upload(&mut self, path: &str, source: &mut File, meta: &Metadata) -> Result<u64> {
        self.target_free(path).await?;
        let staging = format!(
            "{}filemanager-upload-{}.part",
            parent(path),
            uuid::Uuid::new_v4()
        );
        let mut data =
            deadline(async { self.stream.put_with_stream(&staging).await.map_err(message) })
                .await?;
        let result: Result<u64> = async {
            let mut sent = 0;
            let mut buf = vec![0; CHUNK];
            while sent < meta.len() {
                check_cancel()?;
                let n = ((meta.len() - sent).min(CHUNK as u64)) as usize;
                source.read_exact(&mut buf[..n]).map_err(err)?;
                deadline(async { data.write_all(&buf[..n]).await.map_err(err) }).await?;
                sent += n as u64;
                progress(path, sent, meta.len());
            }
            source_unchanged(source, meta, sent)?;
            if self.secure {
                // Keep reading until the peer closes the TLS data channel. Dropping a
                // socket with unread TLS records can reset TCP and truncate the upload.
                deadline(async {
                    data.shutdown().await.map_err(err)?;
                    match data.read(&mut [0]).await {
                        Ok(0) => Ok(()),
                        // Some FTP servers close TCP without returning close_notify.
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(()),
                        Ok(_) => Err("FTPS 上传收到意外的响应数据".into()),
                        Err(e) => Err(err(e)),
                    }
                })
                .await?;
            }
            Ok(sent)
        }
        .await;
        // Consume the FTP completion reply even after cancellation, so the same
        // control connection can remove the staging object.
        let finish =
            deadline(async { self.stream.finalize_put_stream(data).await.map_err(message) }).await;
        let result = async {
            let sent = result?;
            finish?;
            check_cancel()?;
            let stored =
                deadline(async { self.stream.size(&staging).await.map_err(message) }).await? as u64;
            check_size(stored, sent)?;
            self.target_free(path).await?;
            check_cancel()?;
            // FTP has no portable conditional rename. Check again immediately before RNTO.
            deadline(async {
                self.stream
                    .rename(staging.as_str(), path)
                    .await
                    .map_err(message)
            })
            .await?;
            Ok(sent)
        }
        .await;
        match result {
            Ok(sent) => Ok(sent),
            Err(error) => Err(cleanup_message(error, self.delete(&staging, false).await)),
        }
    }

    pub async fn download(&mut self, path: &str, target: &mut File) -> Result<u64> {
        let size = deadline(async { self.stream.size(path).await.map_err(message) }).await? as u64;
        let mut data =
            deadline(async { self.stream.retr_as_stream(path).await.map_err(message) }).await?;
        progress(path, 0, size);
        let mut received = 0;
        let mut buf = vec![0; CHUNK];
        loop {
            check_cancel()?;
            let n = deadline(async { data.read(&mut buf).await.map_err(err) }).await?;
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
        deadline(async {
            self.stream
                .finalize_retr_stream(data)
                .await
                .map_err(message)
        })
        .await?;
        check_size(received, size)?;
        Ok(received)
    }
}

pub(super) struct Reader {
    client: Client,
    data: Option<BufReader<suppaftp::tokio::AsyncDataStream<suppaftp::tokio::AsyncRustlsStream>>>,
    mlsd: bool,
    path: String,
}

impl Reader {
    pub async fn next(&mut self) -> Result<Option<super::pagination::Step>> {
        deadline(async {
            loop {
                let Some(data) = self.data.as_mut() else {
                    return Ok(None);
                };
                let mut line = Vec::new();
                let count = data
                    .take(MAX_LIST_LINE + 1)
                    .read_until(b'\n', &mut line)
                    .await
                    .map_err(err)?;
                if count == 0 {
                    self.client
                        .stream
                        .close_data_connection(self.data.take().unwrap().into_inner())
                        .await
                        .map_err(message)?;
                    return Ok(None);
                }
                if count as u64 > MAX_LIST_LINE {
                    return Err("FTP 目录中存在过长的条目".into());
                }
                let Ok(line) = std::str::from_utf8(&line) else {
                    return Ok(Some(super::pagination::Step::Skipped));
                };
                let line = line.trim_end_matches(['\r', '\n']);
                if line.is_empty() || (!self.mlsd && line.starts_with("total ")) {
                    continue;
                }
                let (parsed, original_name) = if self.mlsd {
                    let Some((facts, name)) = line.split_once("; ") else {
                        return Ok(Some(super::pagination::Step::Skipped));
                    };
                    (ListParser::parse_mlsd(&format!("{facts}; _")), Some(name))
                } else {
                    (line.parse(), None)
                };
                return Ok(Some(match parsed {
                    Ok(entry) => {
                        let name = original_name.unwrap_or_else(|| entry.name());
                        if matches!(name, "." | "..") {
                            continue;
                        }
                        if entry.is_directory() || entry.is_file() {
                            super::pagination::entry(
                                &self.path,
                                name,
                                entry.is_directory(),
                                Some(entry.size() as u64),
                                Some(
                                    chrono::DateTime::<chrono::Utc>::from(entry.modified())
                                        .timestamp_millis(),
                                ),
                            )
                        } else {
                            super::pagination::Step::Skipped
                        }
                    }
                    Err(_) => super::pagination::Step::Skipped,
                }));
            }
        })
        .await
    }
}
