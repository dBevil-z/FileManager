# Remote storage

English · [简体中文](remote-storage.zh-CN.md)

Open **Remote storage → Add connection** in the sidebar. Enter the server and credentials, then use **Test connection** to check that the starting folder can be listed. Save the connection to browse, filter a page, create folders, and upload or download batches of files and directories in the background. Deleting a file or an empty folder requires confirmation. Connections can also be made read only.

| Storage                | Example endpoint                          | Setup                                                                                                                                                                                                                                                                                                               |
| ---------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| MinIO                  | `https://minio.example.com:9000`          | Select MinIO (username and password), enter the API endpoint and local credentials. Bucket and region are optional; path-style addressing is used.                                                                                                                                                                  |
| AWS S3 / S3 compatible | `https://s3.ap-southeast-1.amazonaws.com` | Enter the access key, secret key, and optional session token; choose the required addressing style. Leave bucket blank to list buckets, or region blank for automatic detection.                                                                                                                                    |
| Alibaba Cloud OSS      | `https://oss-cn-hangzhou.aliyuncs.com`    | Select Alibaba Cloud OSS. Enter the regional endpoint, region (`cn-hangzhou`), bucket, AccessKey, and optional STS token. The app adds the bucket subdomain and uses [native OSS V4 signing](https://www.alibabacloud.com/help/en/oss/developer-reference/guidelines-for-upgrading-v1-signatures-to-v4-signatures). |
| SFTP                   | `sftp://files.example.com:22`             | Enter the SSH user and choose an SSH agent, private key file, or password. The host key is checked against the system and user's OpenSSH `known_hosts` before authentication. Verify new fingerprints with the server administrator.                                                                                |
| FTP or explicit FTPS   | `ftps://files.example.com:21`             | Enter the username and password, or use `anonymous`. Connections use passive mode. Explicit FTPS requires `AUTH TLS` and a system-trusted certificate. `ftp://` is unencrypted.                                                                                                                                     |

## MinIO sign-in and all buckets (0.2.2)

Select **MinIO (username and password)**, enter the server's **API** endpoint and credentials, then choose **Sign in and save**. MinIO root and local IAM usernames/passwords are used as S3 access/secret keys ([MinIO user documentation](https://min.io/docs/minio/linux/administration/identity-access-management/minio-user-management.html)). Access-key credentials also work. This entry does not perform LDAP/OIDC browser sign-in.

- **Region blank**: discover the bucket region automatically for default or custom-region MinIO. Enter a region manually only if the service cannot disclose it; an unconfigured region does not require entering `us-east-1`.
- **Bucket blank**: list the buckets the account is authorized to enumerate, with pagination, current-page filtering, entry, and return navigation. Permissions remain enforced by MinIO. When enumeration is unavailable, enter an authorized bucket in the connection or use **Open a bucket directly**.
- **Named bucket**: enter that bucket directly, retaining previous connection behavior. Request parameters cannot switch a fixed-bucket connection to another bucket. In all-buckets mode, directory cursors, transfers, reports, and sync previews retain the selected bucket scope.
- **Starting folder**: blank starts at the bucket root; a prefix applies when entering any bucket. MinIO's API usually uses port `9000`, while the console usually uses `9001`; use your deployment's actual API endpoint.

The sign-in form verifies credentials before saving. Passwords follow the existing session/keyring choice. Enumeration uses S3 ListBuckets and server continuation tokens, bounded to 100,000 buckets and 4 MiB per XML response; connect directly to a bucket if those limits are exceeded. OSS still requires a named bucket and region. Bucket creation/deletion is outside this version.

The endpoint contains only the scheme, host, and optional port. The starting folder is a bucket prefix for object stores. For file servers, leave it blank for the login directory or use an absolute path, including `/` for the server root. Enforce access restrictions through server permissions or bucket policies.

SQLite stores connection settings. Passwords, secret keys, tokens, and private key passphrases stay in memory unless you select **Remember credentials in the system keyring**. That option uses Linux Secret Service; without a working keyring, use the connection for the current run. HTTPS and FTPS use the system CA store.

The Rust core manages connection state and protocol adapters. It uses OpenDAL for S3/OSS, a dedicated SSH adapter for SFTP, and a dedicated FTP adapter for FTP/FTPS. The UI uses the existing Tauri interface and the job queue for transfers and cancellation. This feature runs inside the desktop app, with no separately deployed service or changes to the QuickJS plugin permission model. A separate service would make sense if shared connections, multiple users, or an independent API become requirements.

Object uploads support up to 80,000 MiB (10,000 parts of 8 MiB each). It uses existing buckets and UTF-8 names. Downloads are staged locally and only published after the expected size is received; existing local names are preserved. Uploads check for existing names and use conditional writes or non-overwriting rename where supported. FTP lacks a portable conditional rename, and versioned OSS buckets ignore the no-overwrite condition ([PutObject](https://www.alibabacloud.com/help/en/oss/developer-reference/putobject), [CompleteMultipartUpload](https://www.alibabacloud.com/help/en/oss/developer-reference/completemultipartupload)). Avoid concurrent writes to the same path in those cases. Automatic sync execution, overwrite mirrors, resuming, implicit FTPS, and filesystem mounts are outside this version.

MinIO, SFTP, FTP, and explicit FTPS were exercised locally with empty and 9 MiB files, hash comparisons, name conflicts, and directory operations. The native file picker and keyring persistence across restart were also checked. Automated tests cover OSS V4 requests, credential isolation, aborting S3 multipart uploads, and cleanup after download failure or cancellation. A real Alibaba Cloud bucket still needs acceptance testing.

## Batch transfers (0.2.1)

**Upload file** accepts multiple files. **Upload folders** recursively transfers selected folders, retaining their names, descendants, and empty folders. Check remote items across pages and use **Download selected**. Descendants already covered by a selected parent are deduplicated.

Each selection is limited to 1,000 items; expansion is limited to 100,000 items and 64 levels. The complete scope is scanned before writing. Unreadable remote listings fail planning rather than silently omitting entries. Uploads reject symbolic links and special files. Existing directories can be merged, while existing files are kept. Individual failures do not stop unrelated items; descendants of failed directories are skipped. **View results** shows individual outcomes and a failures-only filter. Cancellation retains completed items and cleans the active temporary file; created empty folders can remain.

Downloads descend through directory file descriptors and reject symlink parents. Uploads verify source identity, size, and modification time against the scan before opening, and check for changes after transfer. Reports expire after 15 minutes and retain at most 8 reports and 200,000 records in total. Status polling returns summaries. Exiting the app interrupts active transfers; remote cleanup depends on connectivity and the server.

## Remote directory pagination (0.2.1)

Directories load 100 entries at a time in server order. **Next** continues the remote cursor beyond the former 5,000-entry limit. S3/OSS use server continuation tokens; SFTP/FTP retain their directory stream position. **Filter this page** searches only the current page.

The last 20 pages are cached for backward navigation. Selection is retained across pages. Refreshing or changing directory starts a new listing. Refresh if the directory changes while browsing. Cursors expire after 5 idle minutes, with at most 16 active cursors; leaving the page releases the active cursor.

## Directory sync preview (0.2.1)

Open the target remote directory, choose **Sync preview**, select a local directory and direction, then start scanning. The preview compares relative paths, sizes, and modification times with a 2-second tolerance. It distinguishes local-only, remote-only, size/time differences, type conflicts, parent conflicts, and unsupported items. Results support status/path filters and pagination. Scanning is cancellable and performs no transfer, overwrite, or deletion.

Equal sizes and similar timestamps do not establish content equality: such files are labeled **Same size; content unverified**. This is a scan snapshot, not a transaction across servers. Exceeding 100,000 items or 64 levels, or encountering an unreadable remote entry, fails the entire comparison rather than inferring missing files from a partial listing. Previews expire after 15 minutes, retaining at most 4 previews and 200,000 entries in total; closing the dialog releases the preview.

Local 0.2.1 checks exercised recursive transfers, empty directories, 9 MiB hash comparisons, continuation after conflicts, symlink protection, and 6,007 entries across cursor pages for each of MinIO, SFTP, FTP, and explicit FTPS. Real cloud endpoints still require acceptance testing.
