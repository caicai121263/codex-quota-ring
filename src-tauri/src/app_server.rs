use crate::quota::{parse_snapshot, QuotaStatus};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, Lines},
    process::Command,
    time::{timeout, Duration},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug)]
pub struct ReadOutcome {
    pub status: QuotaStatus,
    pub candidate_source: String,
}

#[derive(Clone, Debug)]
pub struct ReadError {
    pub code: String,
    pub message: String,
    pub codex_found: bool,
    pub candidate_source: Option<String>,
}

impl ReadError {
    fn new(code: &str, message: &str, codex_found: bool, source: Option<&str>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            codex_found,
            candidate_source: source.map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug)]
struct CodexCandidate {
    path: PathBuf,
    source: &'static str,
}

pub async fn read_quota() -> Result<ReadOutcome, ReadError> {
    let candidates = find_codex_candidates();
    if candidates.is_empty() {
        return Err(ReadError::new(
            "codex_not_found",
            "未找到 Codex。请先安装并登录 Codex 桌面版或 CLI。",
            false,
            None,
        ));
    }

    let mut last_error = None;
    for candidate in candidates {
        match read_from_candidate(&candidate).await {
            Ok(status) => {
                return Ok(ReadOutcome {
                    status,
                    candidate_source: candidate.source.into(),
                });
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        ReadError::new(
            "app_server_spawn_failed",
            "无法启动本机 Codex App Server。",
            true,
            None,
        )
    }))
}

pub fn codex_installation() -> Option<String> {
    find_codex_candidates()
        .first()
        .map(|candidate| candidate.source.to_string())
}

async fn read_from_candidate(candidate: &CodexCandidate) -> Result<QuotaStatus, ReadError> {
    let source = Some(candidate.source);
    let mut command = Command::new(&candidate.path);
    command
        .args(["app-server", "--stdio"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);

    let mut child = command.spawn().map_err(|_| {
        ReadError::new(
            "app_server_spawn_failed",
            "无法启动本机 Codex App Server。",
            true,
            source,
        )
    })?;
    let mut input = child.stdin.take().ok_or_else(|| {
        ReadError::new(
            "app_server_io_failed",
            "无法连接 Codex App Server 输入流。",
            true,
            source,
        )
    })?;
    let output = child.stdout.take().ok_or_else(|| {
        ReadError::new(
            "app_server_io_failed",
            "无法连接 Codex App Server 输出流。",
            true,
            source,
        )
    })?;

    let mut lines = BufReader::new(output).lines();
    let result = timeout(
        REQUEST_TIMEOUT,
        run_protocol(&mut input, &mut lines, source),
    )
    .await
    .map_err(|_| ReadError::new("request_timeout", "读取 Codex 额度超时。", true, source))?;

    let _ = child.kill().await;
    result
}

async fn run_protocol<R, W>(
    input: &mut W,
    lines: &mut Lines<R>,
    source: Option<&str>,
) -> Result<QuotaStatus, ReadError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    send(
        input,
        json!({
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "codex-quota-ring",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
        source,
    )
    .await?;

    let initialize = receive_response(lines, 1, source).await?;
    if initialize.get("error").is_some() {
        return Err(ReadError::new(
            "initialize_failed",
            "Codex App Server 初始化失败。",
            true,
            source,
        ));
    }
    send(
        input,
        json!({"method": "initialized", "params": {}}),
        source,
    )
    .await?;

    send(
        input,
        json!({
            "id": 2,
            "method": "account/read",
            "params": {"refreshToken": false}
        }),
        source,
    )
    .await?;
    let account_response = receive_response(lines, 2, source).await?;
    let account = response_result(&account_response, "account_read_failed", source)?;
    if account_requires_login(account) {
        return Err(ReadError::new(
            "not_logged_in",
            "Codex 尚未登录 ChatGPT 账户。",
            true,
            source,
        ));
    }

    send(
        input,
        json!({
            "id": 3,
            "method": "account/rateLimits/read",
            "params": {}
        }),
        source,
    )
    .await?;
    let limits_response = receive_response(lines, 3, source).await?;
    let limits = response_result(&limits_response, "rate_limits_failed", source)?;
    parse_snapshot(limits, Some(account), now_ms()).map_err(|message| {
        let code = if message.contains("未识别") {
            "unknown_quota_windows"
        } else {
            "missing_quota"
        };
        ReadError::new(code, &message, true, source)
    })
}

fn account_requires_login(account: &Value) -> bool {
    account.get("account").is_some_and(Value::is_null)
        && account
            .get("requiresOpenaiAuth")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn response_result<'a>(
    response: &'a Value,
    code: &str,
    source: Option<&str>,
) -> Result<&'a Value, ReadError> {
    if let Some(result) = response.get("result") {
        return Ok(result);
    }
    let server_message = response
        .get("error")
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = if server_message.to_ascii_lowercase().contains("auth") {
        "Codex 尚未登录，或当前账户无法读取额度。"
    } else {
        "Codex App Server 返回了无法识别的响应。"
    };
    let mapped_code = if server_message.to_ascii_lowercase().contains("auth") {
        "not_logged_in"
    } else {
        code
    };
    Err(ReadError::new(mapped_code, message, true, source))
}

async fn receive_response<R>(
    lines: &mut Lines<R>,
    expected_id: i64,
    source: Option<&str>,
) -> Result<Value, ReadError>
where
    R: AsyncBufRead + Unpin,
{
    while let Some(line) = lines.next_line().await.map_err(|_| {
        ReadError::new(
            "app_server_io_failed",
            "读取 Codex App Server 响应失败。",
            true,
            source,
        )
    })? {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_i64) == Some(expected_id) {
            return Ok(value);
        }
    }
    Err(ReadError::new(
        "app_server_exited",
        "Codex App Server 在返回额度前退出。",
        true,
        source,
    ))
}

async fn send<W>(input: &mut W, value: Value, source: Option<&str>) -> Result<(), ReadError>
where
    W: AsyncWrite + Unpin,
{
    let mut line = serde_json::to_vec(&value).map_err(|_| {
        ReadError::new("protocol_error", "无法生成 App Server 请求。", true, source)
    })?;
    line.push(b'\n');
    input.write_all(&line).await.map_err(|_| {
        ReadError::new(
            "app_server_io_failed",
            "无法向 Codex App Server 发送请求。",
            true,
            source,
        )
    })
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn find_codex_candidates() -> Vec<CodexCandidate> {
    let mut candidates = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.extend(
            find_versioned(Path::new(&local).join("OpenAI").join("Codex").join("bin"))
                .into_iter()
                .map(|path| CodexCandidate {
                    path,
                    source: "Codex Desktop",
                }),
        );
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        candidates.push(CodexCandidate {
            path: Path::new(&profile)
                .join(".codex")
                .join("plugins")
                .join(".plugin-appserver")
                .join("codex.exe"),
            source: "Codex CLI App Server",
        });
    }
    if let Some(path) = find_on_path("codex.exe") {
        candidates.push(CodexCandidate {
            path,
            source: "系统 PATH",
        });
    }
    candidates.retain(|candidate| candidate.path.is_file());
    candidates.dedup_by(|left, right| left.path == right.path);
    candidates
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(';')
        .map(Path::new)
        .map(|folder| folder.join(name))
        .find(|path| path.is_file())
}

fn find_versioned(root: PathBuf) -> Vec<PathBuf> {
    let mut folders: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map(|_| entry.path())
        })
        .collect();
    folders.sort();
    folders.reverse();
    folders
        .into_iter()
        .map(|folder| folder.join("codex.exe"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{duplex, split};

    #[test]
    fn app_server_errors_do_not_include_candidate_paths() {
        let error = ReadError::new(
            "request_timeout",
            "读取 Codex 额度超时。",
            true,
            Some("Codex Desktop"),
        );
        assert!(!error.message.contains("Users"));
        assert!(!error.message.contains('\\'));
        assert_eq!(error.candidate_source.as_deref(), Some("Codex Desktop"));
    }

    #[test]
    fn distinguishes_logged_out_and_logged_in_accounts() {
        assert!(account_requires_login(
            &json!({"account": null, "requiresOpenaiAuth": true})
        ));
        assert!(!account_requires_login(&json!({
            "account": {"type": "chatgpt", "planType": "pro"},
            "requiresOpenaiAuth": true
        })));
        assert!(!account_requires_login(
            &json!({"account": null, "requiresOpenaiAuth": false})
        ));
    }

    #[tokio::test]
    async fn completes_initialize_account_and_rate_limit_sequence() {
        let (client, server) = duplex(8_192);
        let (client_read, mut client_write) = split(client);
        let mut client_lines = BufReader::new(client_read).lines();

        let server_task = tokio::spawn(async move {
            let (server_read, mut server_write) = split(server);
            let mut requests = BufReader::new(server_read).lines();

            let initialize: Value =
                serde_json::from_str(&requests.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(initialize.get("method"), Some(&json!("initialize")));
            server_write
                .write_all(b"{\"id\":1,\"result\":{}}\n")
                .await
                .unwrap();

            let initialized: Value =
                serde_json::from_str(&requests.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(initialized.get("method"), Some(&json!("initialized")));

            let account: Value =
                serde_json::from_str(&requests.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(account.get("method"), Some(&json!("account/read")));
            server_write
                .write_all(b"{\"id\":2,\"result\":{\"account\":{\"type\":\"chatgpt\",\"planType\":\"pro\"},\"requiresOpenaiAuth\":true}}\n")
                .await
                .unwrap();

            let limits: Value =
                serde_json::from_str(&requests.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(
                limits.get("method"),
                Some(&json!("account/rateLimits/read"))
            );
            server_write
                .write_all(b"{\"id\":3,\"result\":{\"rateLimits\":{\"limitId\":\"codex\",\"primary\":{\"usedPercent\":20,\"windowDurationMins\":300},\"secondary\":{\"usedPercent\":40,\"windowDurationMins\":10080}},\"rateLimitResetCredits\":{\"availableCount\":2}}}\n")
                .await
                .unwrap();
        });

        let status = run_protocol(
            &mut client_write,
            &mut client_lines,
            Some("模拟 App Server"),
        )
        .await
        .unwrap();
        server_task.await.unwrap();
        assert_eq!(status.state, "ready");
        assert_eq!(status.five_hour.unwrap().remaining_percent, Some(80.0));
        assert_eq!(status.weekly.unwrap().remaining_percent, Some(60.0));
        assert_eq!(status.credits.unwrap().reset_credits_available, Some(2));
    }

    #[tokio::test]
    async fn reports_early_app_server_exit() {
        let (client, server) = duplex(1024);
        drop(server);
        let (client_read, mut client_write) = split(client);
        let mut client_lines = BufReader::new(client_read).lines();
        let error = run_protocol(
            &mut client_write,
            &mut client_lines,
            Some("模拟 App Server"),
        )
        .await
        .unwrap_err();
        assert!(error.code == "app_server_io_failed" || error.code == "app_server_exited");
    }
}
