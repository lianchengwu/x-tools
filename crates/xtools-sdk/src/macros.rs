#[macro_export]
macro_rules! export_plugin {
    ($plugin_type:ty) => {
        static mut PLUGIN_INSTANCE: Option<$plugin_type> = None;
        #[unsafe(no_mangle)]
        pub extern "C" fn _xtools_alloc(size: u32) -> *mut u8 {
            let mut vec = Vec::<u8>::with_capacity(size as usize);
            let ptr = vec.as_mut_ptr();
            std::mem::forget(vec);
            ptr
        }

        /// 释放传出的缓冲区：ptr 来自本宏的序列化出口（`pack_json_to_host`）
        /// 或 `_xtools_alloc`，两者都保证 capacity == len，因此以
        /// `Vec::from_raw_parts(ptr, size, size)` 重建时释放布局与分配布局一致。
        #[unsafe(no_mangle)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn _xtools_dealloc(ptr: *mut u8, size: u32) {
            if !ptr.is_null() && size > 0 {
                unsafe {
                    let _ = Vec::from_raw_parts(ptr, size as usize, size as usize);
                }
            }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn xtools_plugin_manifest() -> u64 {
            let manifest = <$plugin_type as $crate::XPlugin>::manifest();
            $crate::pack_json_to_host(&manifest)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn xtools_plugin_init() -> u64 {
            match <$plugin_type as $crate::XPlugin>::init() {
                Ok(instance) => {
                    unsafe {
                        PLUGIN_INSTANCE = Some(instance);
                    }
                    let ok_res: Result<(), String> = Ok(());
                    $crate::pack_json_to_host(&ok_res)
                }
                Err(err) => {
                    let err_res: Result<(), String> = Err(err);
                    $crate::pack_json_to_host(&err_res)
                }
            }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn xtools_plugin_render() -> u64 {
            let view = unsafe {
                match &PLUGIN_INSTANCE {
                    Some(instance) => <$plugin_type as $crate::XPlugin>::render(instance),
                    None => $crate::UiView::new($crate::column(vec![
                        $crate::error_label("Plugin not initialized")
                    ])),
                }
            };

            $crate::pack_json_to_host(&view)
        }
        #[unsafe(no_mangle)]
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub extern "C" fn xtools_plugin_handle_event(ptr: *const u8, len: u32) -> u64 {
            if ptr.is_null() || len == 0 {
                return 0;
            }

            let event: Result<$crate::UiEvent, _> = unsafe {
                let slice = std::slice::from_raw_parts(ptr, len as usize);
                serde_json::from_slice(slice)
            };

            let response = match event {
                Ok(evt) => unsafe {
                    match &mut PLUGIN_INSTANCE {
                        Some(instance) => match <$plugin_type as $crate::XPlugin>::handle_event(instance, evt) {
                            Ok(resp) => resp,
                            Err(e) => $crate::UiResponse::ShowToast($crate::Toast {
                                message: format!("Error: {e}"),
                                level: $crate::ToastLevel::Error,
                                duration_ms: 3000,
                            }),
                        },
                        None => $crate::UiResponse::ShowToast($crate::Toast {
                            message: "Plugin instance not initialized".to_string(),
                            level: $crate::ToastLevel::Error,
                            duration_ms: 3000,
                        }),
                    }
                },
                Err(e) => $crate::UiResponse::ShowToast($crate::Toast {
                    message: format!("Invalid event payload: {e}"),
                    level: $crate::ToastLevel::Error,
                    duration_ms: 3000,
                }),
            };

            $crate::pack_json_to_host(&response)
        }
    };
}
