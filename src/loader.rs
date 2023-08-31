// @author:    olinex
// @time:      2023/03/16

// self mods

// use other mods
use core::arch::{asm, global_asm};
use lazy_static::*;

// use self mods
use crate::configs;
use crate::lang::refs;
use crate::println;
use crate::memory::stack::Stack;
use crate::memory::{context, stack};

global_asm!(include_str!("./assembly/link_app.asm"));

struct AppLoader {
    // total count of the applications
    app_count: usize,
    // the index the current running application
    current_app: usize,
    // the start address of the current application
    start_address: usize,
    //
}

impl AppLoader {
    pub fn get_current_app(&self) -> usize {
        self.current_app
    }

    // Get base address of app i.
    fn get_base_address(app_id: usize) -> usize {
        configs::APP_BASE_ADDRESS + app_id * configs::APP_SIZE_LIMIT
    }

    unsafe fn get_address_array(&self) -> &[usize] {
        core::slice::from_raw_parts(self.start_address as *const usize, self.app_count + 1)
    }

    // print all application address range like [start, end)
    fn print_app_infos(&self) {
        println!("[kernel] app count = {}", self.app_count);
        let index_array = unsafe { self.get_address_array() };
        for i in 0..self.app_count {
            let (start_addr, end_addr) = (index_array[i], index_array[i + 1]);
            println!(
                "[kernel] app_{} memory range [{:#x}, {:#x}), length = {:#x}",
                i,
                start_addr as usize,
                end_addr as usize,
                end_addr as usize - start_addr as usize
            );
        }
    }

    fn load_apps(&self) {
        let app_start = unsafe {
            core::slice::from_raw_parts(self.start_address as *const usize, self.app_count + 1)
        };
        // clear i-cache first
        unsafe {
            asm!("fence.i");
        }
        // load apps
        for i in 0..self.app_count {
            let addr = Self::get_base_address(i);
            // clear region
            (addr..addr + configs::APP_SIZE_LIMIT)
                .for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) });
            // load app from data section to memory
            let src = unsafe {
                core::slice::from_raw_parts(
                    app_start[i] as *const u8,
                    app_start[i + 1] - app_start[i],
                )
            };
            let dst = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, src.len()) };
            dst.copy_from_slice(src);
        }
    }

    // load application binary file to memory
    // @app_id: the id of the application
    fn jump_to_app(&self, app_id: usize) -> *const usize {
        if app_id >= self.app_count {
            use crate::boards::qemu::QEMUExit;
            println!("All applications completed!");
            crate::boards::qemu::QEMU_EXIT_HANDLE.exit_success();
        }
        let addr = Self::get_base_address(app_id);
        println!("[kernel] Loading app_{} start from {:#x}", app_id, addr);
        addr as *const usize
    }

    pub fn move_to_next_app(&mut self) {
        self.current_app += 1;
        if self.current_app > self.app_count {
            panic!(
                "[kernel] cannot move to next app out of range: {}",
                self.app_count
            )
        };
    }
}

// app manager should be initialized as static when code was compiling
// and it must be accessible as global symbol by
// but app manager contains
lazy_static! {
    static ref APP_LOADER: refs::UPSafeCell<AppLoader> = unsafe {
        refs::UPSafeCell::new({
            // load _addr_app_count which defined in link_app.asm
            extern "C" { fn _addr_app_count(); }
            // convert _addr_app_count as const usize pointer
            let app_count_ptr = _addr_app_count as usize as *const usize;
            // read app_count value
            let app_count = app_count_ptr.read_volatile();
            if app_count > configs::MAX_APP_NUM {
                panic!("[kernel] application count cannot greater than MAX_APP_NUM: {}", configs::MAX_APP_NUM);
            }
            // initialize app start addresses
            AppLoader {
                app_count,
                current_app: 0,
                start_address: app_count_ptr.add(1) as usize,
            }
        })
    };
}

// init batch subsystem
pub fn init() {
    let loader = APP_LOADER.exclusive_access();
    loader.print_app_infos();
    loader.load_apps()
}

// run next app
// this function must be called in SEE
pub fn run_next_app() -> ! {
    // get real app manager obj from ref cell
    let mut app_manager = APP_LOADER.exclusive_access();
    // get current application index
    let current_app = app_manager.get_current_app();
    // load current app code
    let start_addr = app_manager.jump_to_app(current_app);
    let ctx = context::TrapContext::create_app_init_context(
        start_addr as usize,
        stack::USER_STACK[current_app].get_top(),
    );
    let ctx = stack::KERNEL_STACK[current_app].push_context(ctx);
    app_manager.move_to_next_app();
    // before this we have to drop local variables related to resources manually
    // and release the resources
    drop(app_manager);
    extern "C" {
        fn _fn_restore_all_registers_after_trap(cx_addr: usize);
    }
    unsafe {
        _fn_restore_all_registers_after_trap(ctx as *const _ as usize);
    }
    panic!("Unreachable in batch::run_current_app!");
}
