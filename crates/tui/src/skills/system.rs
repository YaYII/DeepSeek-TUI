//! 系统技能安装器：捆绑 skill-creator 并在首次启动时自动安装。

use std::fs;
use std::path::Path;

const BUNDLED_SKILL_VERSION: &str = "1";
const SKILL_CREATOR_BODY: &str = include_str!("../../assets/skills/skill-creator/SKILL.md");

/// 将捆绑的系统技能安装到 `skills_dir`。
///
/// 行为：
/// - 全新安装（无标记、无目录）：安装 `skill-creator/SKILL.md` 并写入版本标记。
/// - 版本升级（标记存在但版本较旧，目录存在）：重新安装。
/// - 用户在标记仍存在且版本相同时删除了目录：保持删除状态。
/// - 幂等：在无变化的情况下调用两次是无操作的。
///
/// 错误是来自文件系统的 I/O 错误；调用者应记录它们但不要中止启动。
pub fn install_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    let marker = skills_dir.join(".system-installed-version");
    let target_dir = skills_dir.join("skill-creator");
    let target_file = target_dir.join("SKILL.md");

    let installed_version = fs::read_to_string(&marker)
        .ok()
        .map(|s| s.trim().to_string());
    let dir_exists = target_dir.exists();

    // 仅在两个条件同时成立时重新安装：
    //   (a) 捆绑版本比标记中记录的版本更新，且
    //   (b) 技能目录仍然存在（用户未有意删除）。
    // 全新安装（无标记且无目录）也会被处理。
    let should_install = match (installed_version.as_deref(), dir_exists) {
        // 全新安装：既没有标记也没有目录。
        (None, false) => true,
        // 版本升级：标记过时但目录仍然存在。
        (Some(v), true) if v != BUNDLED_SKILL_VERSION => true,
        // 所有其他情况：已在当前版本安装，或用户删除了目录（尊重该选择）。
        _ => false,
    };

    if should_install {
        fs::create_dir_all(skills_dir)?;
        fs::create_dir_all(&target_dir)?;
        fs::write(&target_file, SKILL_CREATOR_BODY)?;
        fs::write(&marker, BUNDLED_SKILL_VERSION)?;
    }
    Ok(())
}

/// 移除 `skill-creator` 系统技能及其版本标记。
///
/// 用于测试和 `deepseek setup --clean`。忽略缺失的文件。
#[allow(dead_code)]
pub fn uninstall_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    let marker = skills_dir.join(".system-installed-version");
    let target_dir = skills_dir.join("skill-creator");

    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)?;
    }
    if marker.exists() {
        fs::remove_file(&marker)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── 辅助函数 ──────────────────────────────────────────────────────────────

    fn skill_file(tmp: &TempDir) -> std::path::PathBuf {
        tmp.path().join("skill-creator").join("SKILL.md")
    }

    fn marker_file(tmp: &TempDir) -> std::path::PathBuf {
        tmp.path().join(".system-installed-version")
    }

    // ── 全新安装 ─────────────────────────────────────────────────────────

    #[test]
    fn fresh_install_creates_skill_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        assert!(skill_file(&tmp).exists(), "SKILL.md 应被创建");
        assert!(marker_file(&tmp).exists(), "标记应被创建");

        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    // ── 幂等性 ───────────────────────────────────────────────────────────

    #[test]
    fn calling_twice_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        // 用哨兵值覆盖 SKILL.md 来检测非预期的二次写入。
        fs::write(skill_file(&tmp), "sentinel").unwrap();

        install_system_skills(tmp.path()).unwrap();

        let contents = fs::read_to_string(skill_file(&tmp)).unwrap();
        assert_eq!(
            contents, "sentinel",
            "第二次安装不应在版本相同时覆盖 SKILL.md"
        );
    }

    // ── 用户删除了目录 ────────────────────────────────────────────

    #[test]
    fn user_deleted_dir_is_not_recreated() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        // 模拟用户有意删除技能目录。
        fs::remove_dir_all(tmp.path().join("skill-creator")).unwrap();

        // 重新启动不得重新创建该目录。
        install_system_skills(tmp.path()).unwrap();

        assert!(
            !skill_file(&tmp).exists(),
            "用户删除后 skill-creator 不得被重新创建"
        );
    }

    // ── 版本升级重新安装 ──────────────────────────────────────────────

    #[test]
    fn outdated_marker_triggers_reinstall() {
        let tmp = TempDir::new().unwrap();

        // 模拟先前在较低版本的安装。
        let skill_dir = tmp.path().join("skill-creator");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "old content").unwrap();
        fs::write(marker_file(&tmp), "0").unwrap(); // 比 BUNDLED_SKILL_VERSION 更旧

        install_system_skills(tmp.path()).unwrap();

        let contents = fs::read_to_string(skill_file(&tmp)).unwrap();
        assert_ne!(
            contents, "old content",
            "过时的技能应在版本升级时被覆盖"
        );
        assert_eq!(
            contents, SKILL_CREATOR_BODY,
            "重新安装的文件必须与捆绑内容匹配"
        );

        let ver = fs::read_to_string(marker_file(&tmp)).unwrap();
        assert_eq!(
            ver.trim(),
            BUNDLED_SKILL_VERSION,
            "标记应被更新"
        );
    }

    // ── 卸载 ─────────────────────────────────────────────────────────────

    #[test]
    fn uninstall_removes_skill_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        uninstall_system_skills(tmp.path()).unwrap();

        assert!(!skill_file(&tmp).exists(), "SKILL.md 应被移除");
        assert!(!marker_file(&tmp).exists(), "标记应被移除");
    }

    #[test]
    fn uninstall_on_clean_dir_is_a_noop() {
        let tmp = TempDir::new().unwrap();
        // 不得 panic 或出错。
        uninstall_system_skills(tmp.path()).unwrap();
    }
}
