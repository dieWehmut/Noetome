use super::*;
use std::{ffi::OsString, path::PathBuf};

pub const HELP: &str = "整书PDF资料库（与桌面共用持久进度）\n\nnoetome corpus preflight <PDF>\nnoetome corpus import <项目> <PDF>\nnoetome corpus list <项目>\nnoetome corpus status <项目> <source-id>\nnoetome corpus process <项目> <source-id>\nnoetome corpus retry <项目> <source-id>\nnoetome corpus pause <项目> <source-id>\nnoetome corpus pages <项目> <source-id> <offset> <limit>\nnoetome corpus export-source <项目> <source-id> <新目录>\n\nimport流式获取并登记全部物理页，process解析未确认页；暂停或退出后可继续。\n整书无固定大小/总页数上限；每批8页/180秒、单页结构16MiB、查询每次1到50页。\n扫描、复杂布局和失败页分别保留问题，导入不代表知识已抽取或科学正确。\n";

#[derive(Debug)]
pub enum Command {
    Help,
    Preflight(PathBuf),
    Import(PathBuf, PathBuf),
    List(PathBuf),
    Status(PathBuf, String),
    Process(PathBuf, String),
    Retry(PathBuf, String),
    Pause(PathBuf, String),
    Pages(PathBuf, String, u32, u32),
    ExportSource(PathBuf, String, PathBuf),
}
pub fn parse(args: &[OsString]) -> Result<Command, &'static str> {
    let invalid = crate::INVALID_ARGUMENT;
    let value = |s: &OsString| {
        s.to_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or(invalid)
    };
    match args {
        [] => Ok(Command::Help),
        [help] if help == "--help" || help == "-h" => Ok(Command::Help),
        [verb, path] if verb == "preflight" => Ok(Command::Preflight(PathBuf::from(value(path)?))),
        [verb, project, path] if verb == "import" => Ok(Command::Import(
            PathBuf::from(value(project)?),
            PathBuf::from(value(path)?),
        )),
        [verb, project] if verb == "list" => Ok(Command::List(PathBuf::from(value(project)?))),
        [verb, project, id] if verb == "status" => {
            Ok(Command::Status(PathBuf::from(value(project)?), value(id)?))
        }
        [verb, project, id] if verb == "process" => {
            Ok(Command::Process(PathBuf::from(value(project)?), value(id)?))
        }
        [verb, project, id] if verb == "retry" => {
            Ok(Command::Retry(PathBuf::from(value(project)?), value(id)?))
        }
        [verb, project, id] if verb == "pause" => {
            Ok(Command::Pause(PathBuf::from(value(project)?), value(id)?))
        }
        [verb, project, id, destination] if verb == "export-source" => Ok(Command::ExportSource(
            PathBuf::from(value(project)?),
            value(id)?,
            PathBuf::from(value(destination)?),
        )),
        [verb, project, id, offset, limit] if verb == "pages" => Ok(Command::Pages(
            PathBuf::from(value(project)?),
            value(id)?,
            value(offset)?.parse().map_err(|_| invalid)?,
            value(limit)?.parse().map_err(|_| invalid)?,
        )),
        _ => Err(invalid),
    }
}
pub fn execute(command: Command) -> Result<Value, ProjectError> {
    let serialize =
        |v: SourceSummary| serde_json::to_value(v).map_err(|e| error("CORPUS_JSON", e.to_string()));
    match command {
        Command::Help => Ok(json!({"help":HELP})),
        Command::Preflight(path) => preflight(&path),
        Command::Import(project, path) => serialize(import(&project, &path)?),
        Command::List(project) => Ok(json!({"sources":list(&project)?})),
        Command::Status(project, id) => serialize(status(&project, &id)?),
        Command::Process(project, id) => {
            serialize(process(&project, &id, Arc::new(AtomicBool::new(false)))?)
        }
        Command::Retry(project, id) => {
            serialize(retry(&project, &id, Arc::new(AtomicBool::new(false)))?)
        }
        Command::Pause(project, id) => serialize(pause(&project, &id)?),
        Command::Pages(project, id, offset, limit) => pages(&project, &id, offset, limit),
        Command::ExportSource(project, id, destination) => {
            export_source(&project, &id, &destination)
        }
    }
}
