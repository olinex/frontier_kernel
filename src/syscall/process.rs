//! App management syscalls
use crate::task::exit_current_and_run_other_task;
use crate::println;

// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    println!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_other_task();
    panic!("Unreachable in sys_exit!");
}
