use super::*;
use g7_system::command::{CommandOutput, FakeCommandRunner};
use std::ffi::OsString;

#[test]
fn swapfile_commands_create_without_shell_and_fallback_to_dd()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::failure(1, "fallocate unavailable"));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success("/swapfile\n"));
    let probe = SystemProbe::new(runner);
    let sizing = plan::resolve_memory_sizing(2 * 1024 * 1024, 2);

    apply_swapfile_system_commands(&probe, &sizing, false)?;

    let recorded = probe.runner().recorded();
    assert!(recorded.iter().all(|command| command.program != "sh"));
    assert_eq!(recorded[0].program, OsString::from("fallocate"));
    assert_eq!(recorded[1].program, OsString::from("dd"));
    assert!(recorded[1].args.contains(&OsString::from("of=/swapfile")));
    assert!(recorded[1].args.contains(&OsString::from("count=2048")));
    assert_eq!(recorded[2].display(), "chmod 600 /swapfile");
    assert_eq!(recorded[3].display(), "mkswap /swapfile");
    assert_eq!(recorded[4].display(), "systemctl daemon-reload");
    assert_eq!(
        recorded[5].display(),
        "systemctl enable --now swapfile.swap"
    );
    assert_eq!(recorded[6].display(), "sysctl --system");
    assert_eq!(recorded[7].display(), "swapon --show=NAME");
    Ok(())
}

#[test]
fn swapfile_commands_reuse_existing_file_without_reformatting()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success("/swapfile\n"));
    let probe = SystemProbe::new(runner);
    let sizing = plan::resolve_memory_sizing(2 * 1024 * 1024, 2);

    apply_swapfile_system_commands(&probe, &sizing, true)?;

    let recorded = probe.runner().recorded();
    assert!(recorded.iter().all(|command| command.program != "sh"));
    assert_eq!(recorded[0].display(), "chmod 600 /swapfile");
    assert!(
        recorded
            .iter()
            .all(|command| command.program != OsString::from("mkswap"))
    );
    Ok(())
}

#[test]
fn swapfile_verify_requires_active_swapfile() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    let probe = SystemProbe::new(runner);
    let sizing = plan::resolve_memory_sizing(2 * 1024 * 1024, 2);

    let error = apply_swapfile_system_commands(&probe, &sizing, true).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("/swapfile is missing from swapon --show=NAME")
    );
    Ok(())
}
