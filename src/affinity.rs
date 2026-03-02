#[cfg(target_os = "linux")]
pub fn set_thread_affinity(core_id: usize) -> Result<(), i32> {
    use nix::sched::{sched_setaffinity, CpuSet};
    use nix::unistd::Pid;
    let mut cpuset = CpuSet::new();
    cpuset.set(core_id).map_err(|_| -1)?;
    sched_setaffinity(Pid::from_raw(0), &cpuset).map_err(|e| e as i32)
}

#[cfg(target_os = "windows")]
pub fn set_thread_affinity(core_id: usize) -> Result<(), i32> {
    let _ = core_id;
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn set_thread_affinity(_core_id: usize) -> Result<(), i32> {
    Ok(())
}
