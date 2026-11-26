#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("shim.h");
        fn ressac_init_webengine();
        fn ressac_schedule_grab(ms: i32, path: &str);
        fn ressac_schedule_quit(ms: i32);
        fn ressac_create_qapplication();
        fn ressac_setup_icons();
        fn ressac_exec() -> i32;
    }
}
