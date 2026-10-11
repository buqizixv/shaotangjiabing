//! Host-only batch bridge. Script apps still cannot call calendar.*.
use super::*;
pub fn events(host_dir: &Path) -> Result<Vec<Event>, String> {
    let _guard = STORE.lock().unwrap_or_else(|e|e.into_inner());
    read(host_dir)
}
fn read(host_dir: &Path) -> Result<Vec<Event>, String> {
    match std::fs::read(store_path(host_dir)) {
        Ok(bytes)=>serde_json::from_slice(&bytes).map_err(|_|"系统日历数据损坏，请先修复；未覆盖原文件。".into()),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(vec![]),
        Err(e)=>Err(format!("无法读取系统日历：{e}")),
    }
}
fn belongs(event: &Event, id: &str)->bool {
    event.id.strip_prefix(&format!("daycast-{id}-")).is_some_and(|suffix|suffix.parse::<usize>().is_ok())
}
pub fn sync(host_dir: &Path, id: &str, rows: &[Event], confirmed: bool) -> Result<(),String> {
    sync_replacing(host_dir,id,rows,confirmed,"")
}
pub fn sync_replacing(host_dir: &Path, id: &str, rows: &[Event], confirmed: bool, replace:&str) -> Result<(),String> {
    if !confirmed || id.is_empty() || !id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-') || rows.is_empty() || rows.len()>12 {return Err("请先确认完整行程。".into());}
    let _guard = STORE.lock().unwrap_or_else(|e|e.into_inner());
    let mut stored=read(host_dir)?;
    if !replace.is_empty() && (!replace.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-') || replace==id) {return Err("无效的原行程标识。".into());}
    let prefix=format!("daycast-{id}-");
    for (i,e) in rows.iter().enumerate() {
        if e.id!=format!("{prefix}{i}") || e.title.is_empty() || e.title.chars().count()>80 || e.location.chars().count()>120 || e.notes.chars().count()>12000
            || e.end.as_ref().is_none_or(|end|parse_time(end).is_err()||parse_time(&e.start).is_err()||end<=&e.start) {return Err("系统日历行程格式不完整。".into());}
    }
    let existing:Vec<_>=stored.iter().filter(|e|belongs(e,id)).collect();
    if !existing.is_empty() {
        if existing.len()!=rows.len() || !rows.iter().all(|e|existing.iter().any(|s|*s==e)) {return Err("系统日历里的该行程已被修改或删除，请核对后新建版本。".into());}
        return Ok(());
    }
    if !replace.is_empty() {
        if !stored.iter().any(|e|belongs(e,replace)) {return Err("系统日历原行程已不存在，请核对后重试。".into());}
        stored.retain(|e|!belongs(e,replace));
    }
    if stored.len()+rows.len()>EVENTS_MAX {return Err("系统日历容量不足，未同步行程。".into());}
    for row in rows {
        if let Some(e)=stored.iter().find(|e|e.start<*row.end.as_ref().unwrap() && row.start<e.end.clone().unwrap_or(e.start.clone())) {return Err(format!("与系统日历“{}”冲突，未同步任何安排。",e.title));}
    }
    stored.extend_from_slice(rows);stored.sort_by(|a,b|a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    save(host_dir,&stored)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().ok_or("无效的日程路径")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建日程目录：{e}"))?;
    let temp = dir.join(format!(".schedule-{}-{}.tmp", std::process::id(), Local::now().timestamp_nanos_opt().unwrap_or_default()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| format!("无法保存日程：{e}"))?;
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("无法保存日程：{e}"))?;
        drop(file);
        #[cfg(windows)]
        if path.exists() {
            use std::os::windows::ffi::OsStrExt;
            #[link(name="kernel32")]
            extern "system" { fn ReplaceFileW(replaced:*const u16,replacement:*const u16,backup:*const u16,flags:u32,exclude:*mut std::ffi::c_void,reserved:*mut std::ffi::c_void)->i32; }
            let old:Vec<_>=path.as_os_str().encode_wide().chain(Some(0)).collect();
            let new:Vec<_>=temp.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: these NUL-terminated buffers live through the call;
            // optional pointer parameters are null, as documented by Win32.
            if unsafe { ReplaceFileW(old.as_ptr(),new.as_ptr(),std::ptr::null(),0,std::ptr::null_mut(),std::ptr::null_mut()) } == 0 {
                return Err(format!("无法替换日程，原文件已保留：{}",std::io::Error::last_os_error()));
            }
            return Ok(());
        }
        std::fs::rename(&temp,path).map_err(|e| format!("无法保存日程，原文件已保留：{e}"))
    })();
    if result.is_err() { let _=std::fs::remove_file(temp); }
    result
}
