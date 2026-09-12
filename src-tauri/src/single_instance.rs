use fs2::FileExt;
use std::{fs, fs::File, fs::OpenOptions, path::Path};
use tauri::{App, Manager};

pub struct SingleInstanceGuard {
    _file: File,
}

fn lock_file(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("无法创建单实例锁：{e}"))?;
    file.try_lock_exclusive()
        .map_err(|_| "物资管理系统已经在运行，请切换到现有窗口".to_string())?;
    Ok(file)
}

pub fn acquire(app: &App) -> Result<(), String> {
    let config_dir = app
        .path_resolver()
        .app_config_dir()
        .ok_or_else(|| "无法获取应用数据目录".to_string())?;
    fs::create_dir_all(&config_dir).map_err(|e| format!("无法创建应用数据目录：{e}"))?;
    let guard = SingleInstanceGuard {
        _file: lock_file(&config_dir.join("kylin-stock.lock"))?,
    };
    app.manage(guard);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_second_lock_until_the_first_is_dropped() {
        let path = std::env::temp_dir().join(format!(
            "kylinstock-single-instance-{}.lock",
            std::process::id()
        ));
        let first = lock_file(&path).expect("first instance should acquire lock");
        let error = lock_file(&path).expect_err("second instance must be rejected");
        assert!(error.contains("已经在运行"));
        drop(first);
        lock_file(&path).expect("lock should be released after process guard is dropped");
        let _ = fs::remove_file(path);
    }
}
