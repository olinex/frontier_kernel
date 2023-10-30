// @author:    olinex
// @time:      2023/10/13

// self mods


// use other mods
use core::str::FromStr;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;

// use self mods
use crate::constant::ascii;
use crate::lang::container::UserPromiseRefCell;
use crate::prelude::*;
use crate::sbi::*;

#[repr(C)]
struct AppRange {
    /// [start, end)
    pub start: usize,
    pub end: usize,
}

pub struct AppLoader {
    applications: BTreeMap<String, &'static [u8]>,
}
impl AppLoader {
    unsafe fn load_name(ptr: *const u8) -> Result<(&'static str, *const u8)> {
        let mut end = ptr;
        while end.read_volatile() != ascii::NULL {
            end = end.add(1);
        }
        let slice = core::slice::from_ptr_range(ptr..end);
        Ok((core::str::from_utf8(slice)?, end.add(1)))
    }

    fn new() -> Self {
        Self {
            applications: BTreeMap::new(),
        }
    }

    fn push(&mut self, name: &str, data: &'static [u8]) -> Result<()> {
        let name = String::from_str(name)?;
        if let Some(_) = self.applications.insert(name.clone(), data) {
            Err(KernelError::ProcessAlreadyExists(name))
        } else {
            Ok(())
        }
    }

    fn get(&self, name: &str) -> Result<&'static [u8]> {
        let name = String::from_str(name)?;
        let data = self
            .applications
            .get(&name)
            .ok_or(KernelError::ProcessDoesNotExists(name))?;
        Ok(*data)
    }
}

lazy_static! {
    pub static ref APP_LOADER: Arc<UserPromiseRefCell<AppLoader>> = {
        // load _addr_app_count which defined in link_app.asm
        extern "C" {
            fn _addr_app_count();
            fn _app_names();
        }
        let mut app_name_ptr = _app_names as usize as *const u8;
        // convert _addr_app_count as const usize pointer
        let task_count_ptr = _addr_app_count as usize as *const usize;
        // read app_count value
        let task_count = unsafe {task_count_ptr.read_volatile()};
        // get start address which is after the app count pointer
        let task_range_ptr = unsafe {task_count_ptr.add(1)} as usize;
        // load task range slice
        let task_ranges = unsafe {
            core::slice::from_raw_parts(task_range_ptr as *const AppRange, task_count)
        };
        // create loader
        let mut loader = AppLoader::new();

        // clear i-cache first
        unsafe { SBI::sync_icache() };

        // load apps
        debug!("task count = {}", task_ranges.len());
        for (i, range) in task_ranges.iter().enumerate() {
            // load app from data section to memory
            let length = range.end - range.start;
            let (name, new_app_name_ptr) = unsafe {AppLoader::load_name(app_name_ptr).unwrap()};
            // load task's code in byte slice
            let src = unsafe { core::slice::from_raw_parts(range.start as *const u8, length) };
            // create a new
            loader.push(name, src).unwrap();
            // move app name's pointer to the next one
            app_name_ptr = new_app_name_ptr;
            debug!(
                "app_{} memory range [{:#x}, {:#x}), length = {:#x}, name = {}",
                i, range.start, range.end, length, name
            );
            // load task's name in &str
        }
        Arc::new(unsafe {UserPromiseRefCell::new(loader)})
    };
}
impl APP_LOADER {
    pub fn get(&self, name: &str) -> Result<&'static [u8]> {
        self.access().get(name)
    }
}
