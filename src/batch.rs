// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use core::arch::{asm, global_asm};
use lazy_static::*;

// use self mods
use crate::language::refs::UPSafeCell;
use crate::println;
use crate::trap::stack::Stack;
use crate::trap::{context::TrapContext, stack::KERNEL_STACK, stack::USER_STACK};

const MAX_APP_NUM: usize = 16;
const APP_BASE_ADDRESS: usize = 0x80400000;
const APP_SIZE_LIMIT: usize = 0x20000;

global_asm!(include_str!("./assemble/link_app.asm"));

struct AppManager {
    // total count of the applications
    app_count: usize,
    // the address of the current running application
    current_app: usize,
    // the addresses of the applications
    // the length of it must be equal to app_count + 1
    app_start_addresses: [usize; MAX_APP_NUM + 1],
}

impl AppManager {
    // print all application address range like [start, end)
    fn print_app_infos(&self) {
        println!("[kernel] app count = {}", self.app_count);
        for i in 0..self.app_count {
            println!(
                "[kernel] app_{} [{:#x}, {:#x})",
                i,
                self.app_start_addresses[i],
                self.app_start_addresses[i + 1]
            );
        }
    }

    // load application binary file to memory
    // @app_id: the id of the application
    unsafe fn load_app(&self, app_id: usize) {
        if app_id >= self.app_count {
            use crate::boards::qemu::QEMUExit;
            println!("All applications completed!");
            crate::boards::qemu::QEMU_EXIT_HANDLE.exit_success();
        }
        println!("[kernel] Loading app_{}", app_id);
        // clear app area
        core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, APP_SIZE_LIMIT).fill(0);
        let app_src = core::slice::from_raw_parts(
            self.app_start_addresses[app_id] as *const u8,
            self.app_start_addresses[app_id + 1] - self.app_start_addresses[app_id],
        );
        let app_dst = core::slice::from_raw_parts_mut(APP_BASE_ADDRESS as *mut u8, app_src.len());
        app_dst.copy_from_slice(app_src);
        // Memory fence about fetching the instruction memory
        // It is guaranteed that a subsequent instruction fetch must
        // observes all previous writes to the instruction memory.
        // Therefore, fence.i must be executed after we have loaded
        // the code of the next app into the instruction memory.
        // See also: riscv non-priv spec chapter 3, 'Zifencei' extension.
        asm!("fence.i");
    }

    pub(crate) fn get_current_app(&self) -> usize {
        self.current_app
    }

    pub(crate) fn move_to_next_app(&mut self) {
        self.current_app += 1;
        if self.current_app > self.app_count {
            panic!("[kernel] cannot move to next app out of range: {}", self.app_count)
        };
    }
}

// app manager should be initialized as static when code was compiling
// and it must be accessible as global symbol by
// but app manager contains 
lazy_static! {
    static ref APP_MANAGER: UPSafeCell<AppManager> = unsafe {
        UPSafeCell::new({
            // load _addr_app_count which defined in link_app.asm
            extern "C" {
                fn _addr_app_count();
            }
            // convert _addr_app_count as const usize pointer
            let app_count_ptr = _addr_app_count as usize as *const usize;
            // read app_count value
            let app_count = app_count_ptr.read_volatile();
            if app_count > MAX_APP_NUM {
                panic!("[kernel] application count cannot greater than MAX_APP_NUM: {}", MAX_APP_NUM);
            }
            // initialize app start addresses list
            let mut app_start_addresses: [usize; MAX_APP_NUM + 1] = [0; MAX_APP_NUM + 1];
            // in link_app.asm, app start addresses was defined just after _addr_app_count
            // so app_start_addresses[0] = app_count_ptr.add(1)
            let app_start_raw: &[usize] = core::slice::from_raw_parts(
                app_count_ptr.add(1),
                app_count + 1
            );
            // copy raw values to app_start_addresses
            app_start_addresses[..=app_count].copy_from_slice(app_start_raw);
            AppManager {
                app_count,
                current_app: 0,
                app_start_addresses,
            }
        })
    };
}

/// init batch subsystem
pub(crate) fn init() {
    print_app_infos();
}

/// print apps info
pub(crate) fn print_app_infos() {
    APP_MANAGER.exclusive_access().print_app_infos();
}

/// run next app
pub(crate) fn run_next_app() -> ! {
    // get real app manager obj from ref cell
    let mut app_manager = APP_MANAGER.exclusive_access();
    // get current application index
    let current_app = app_manager.get_current_app();
    // load current app code 
    unsafe {
        app_manager.load_app(current_app);
    }

    app_manager.move_to_next_app();
    drop(app_manager);
    // before this we have to drop local variables related to resources manually
    // and release the resources
    extern "C" {
        fn _addr_restore_all_registers_after_trap(cx_addr: usize);
    }
    let ctx = TrapContext::create_app_init_context(APP_BASE_ADDRESS, USER_STACK.get_top());
    let ctx = KERNEL_STACK.push_context(ctx);
    unsafe {
        _addr_restore_all_registers_after_trap(ctx as *const _ as usize);
    }
    panic!("Unreachable in batch::run_current_app!");
}
