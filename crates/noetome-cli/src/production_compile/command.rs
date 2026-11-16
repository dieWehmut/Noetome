use super::*;
use std::{ffi::OsString, path::PathBuf};

pub const HELP: &str = "Noetome 真实来源持久编译\n\ncompile create <项目> <来源ID数组.json> <编译配置.json>\ncompile run <项目> <run-id>\ncompile list <项目>\ncompile status <项目> <run-id>\ncompile control <项目> <run-id> <pause|cancel|retry>\ncompile query <项目> <run-id> <concepts|atoms|evidence|issues|pages|units> [offset] [limit] [search]\ncompile evidence <项目> <run-id> <evidence-id>\ncompile export <项目> <run-id> <不存在的新Vault>\ncompile budget <migrate|status> <原累计账本.json>\n\n范围为全部物理页，单请求有界；确认知识为未核验草稿。pages与units范围分页不接受正文搜索。恢复不重置累计预算，来源包和知识笔记分别导出。\n";

#[derive(Debug, Clone)]
pub enum Command {
    Help,
    Create(PathBuf, PathBuf, PathBuf),
    Run(PathBuf, String),
    List(PathBuf),
    Status(PathBuf, String),
    Control(PathBuf, String, String),
    Query(PathBuf, String, String, u64, u32, String),
    Evidence(PathBuf, String, String),
    Export(PathBuf, String, PathBuf),
    Budget(String, PathBuf),
}
pub fn parse(args: &[OsString]) -> Result<Command, &'static str> {
    let bad = crate::INVALID_ARGUMENT;
    if args.is_empty() || (args.len() == 1 && (args[0] == "--help" || args[0] == "-h")) {
        return Ok(Command::Help);
    }
    let verb = args[0].to_str().ok_or(bad)?;
    let rest = &args[1..];
    let text = |i: usize| -> Result<String, &'static str> {
        rest.get(i)
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or(bad)
    };
    let path = |i: usize| -> Result<PathBuf, &'static str> {
        rest.get(i)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or(bad)
    };
    match (verb, rest.len()) {
        ("create", 3) => Ok(Command::Create(path(0)?, path(1)?, path(2)?)),
        ("run", 2) => Ok(Command::Run(path(0)?, text(1)?)),
        ("list", 1) => Ok(Command::List(path(0)?)),
        ("status", 2) => Ok(Command::Status(path(0)?, text(1)?)),
        ("control", 3) => Ok(Command::Control(path(0)?, text(1)?, text(2)?)),
        ("query", n) if (3..=6).contains(&n) => Ok(Command::Query(
            path(0)?,
            text(1)?,
            text(2)?,
            if n > 3 {
                text(3)?.parse().map_err(|_| bad)?
            } else {
                0
            },
            if n > 4 {
                text(4)?.parse().map_err(|_| bad)?
            } else {
                20
            },
            if n > 5 { text(5)? } else { String::new() },
        )),
        ("evidence", 3) => Ok(Command::Evidence(path(0)?, text(1)?, text(2)?)),
        ("export", 3) => Ok(Command::Export(path(0)?, text(1)?, path(2)?)),
        ("budget", 2) => Ok(Command::Budget(text(0)?, path(1)?)),
        _ => Err(bad),
    }
}
pub fn execute(command: Command) -> Result<String, ProjectError> {
    let value = match command {
        Command::Help => return Ok(HELP.into()),
        Command::Create(project, selection, config) => {
            let sources: Vec<String> =
                serde_json::from_slice(&read_artifact(&selection, 65536)?)
                    .map_err(|_| error("COMPILE_SELECTION", "来源ID选择不是JSON数组。"))?;
            let config: CompileConfig = serde_json::from_slice(&read_artifact(&config, 65536)?)
                .map_err(|_| error("COMPILE_CONFIG", "编译配置不可读。"))?;
            serde_json::to_value(create(&project, &sources, config)?)
        }
        Command::Run(project, id) => {
            serde_json::to_value(run(&project, &id, Arc::new(AtomicBool::new(false)))?)
        }
        Command::List(project) => serde_json::to_value(list(&project)?),
        Command::Status(project, id) => serde_json::to_value(status(&project, &id)?),
        Command::Control(project, id, action) => {
            serde_json::to_value(control(&project, &id, &action)?)
        }
        Command::Query(project, id, kind, offset, limit, search) => {
            Ok(query(&project, &id, &kind, offset, limit, &search)?)
        }
        Command::Evidence(project, id, eid) => Ok(evidence(&project, &id, &eid)?),
        Command::Export(project, id, output) => Ok(export(&project, &id, &output)?),
        Command::Budget(action, ledger) => {
            let mut authority = match action.as_str() {
                "migrate" => BudgetAuthority::migrate(&ledger)?,
                "status" => BudgetAuthority::open(&ledger)?,
                _ => return Err(error("COMPILE_BUDGET", "预算支持migrate或status。")),
            };
            serde_json::to_value(authority.status()?)
        }
    }
    .map_err(|_| error("COMPILE_ENCODING", "命令结果编码失败。"))?;
    serde_json::to_string_pretty(&value)
        .map(|s| format!("{s}\n"))
        .map_err(|_| error("COMPILE_ENCODING", "命令结果编码失败。"))
}
