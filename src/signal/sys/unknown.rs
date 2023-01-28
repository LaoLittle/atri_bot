pub fn init_crash_handler() {
    ::tracing::warn!("当前系统暂未支持处理插件异常");
}

pub unsafe fn save_jmp() {

}

pub fn exception_jmp(status: std::ffi::c_int) -> ! {
    std::process::exit(status);
}
