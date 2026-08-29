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

        #[unsafe(no_mangle)]
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
            match serde_json::to_vec(&manifest) {
                Ok(mut bytes) => {
                    let ptr = bytes.as_mut_ptr();
                    let len = bytes.len() as u32;
                    std::mem::forget(bytes);
                    $crate::pack_ptr_len(ptr as u32, len)
                }
                Err(_) => 0,
            }
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn xtools_plugin_init() -> u64 {
            let init_result = <$plugin_type as $crate::XPlugin>::init();
            match init_result {
                Ok(instance) => {
                    unsafe {
                        PLUGIN_INSTANCE = Some(instance);
                    }
                    let ok_res: Result<(), String> = Ok(());
                    let mut bytes = serde_json::to_vec(&ok_res).unwrap_or_default();
                    let ptr = bytes.as_mut_ptr();
                    let len = bytes.len() as u32;
                    std::mem::forget(bytes);
                    $crate::pack_ptr_len(ptr as u32, len)
                }
                Err(err) => {
                    let err_res: Result<(), String> = Err(err);
                    let mut bytes = serde_json::to_vec(&err_res).unwrap_or_default();
                    let ptr = bytes.as_mut_ptr();
                    let len = bytes.len() as u32;
                    std::mem::forget(bytes);
                    $crate::pack_ptr_len(ptr as u32, len)
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

            match serde_json::to_vec(&view) {
                Ok(mut bytes) => {
                    let ptr = bytes.as_mut_ptr();
                    let len = bytes.len() as u32;
                    std::mem::forget(bytes);
                    $crate::pack_ptr_len(ptr as u32, len)
                }
                Err(_) => 0,
            }
        }
        #[unsafe(no_mangle)]
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

            match serde_json::to_vec(&response) {
                Ok(mut bytes) => {
                    let ptr = bytes.as_mut_ptr();
                    let len = bytes.len() as u32;
                    std::mem::forget(bytes);
                    $crate::pack_ptr_len(ptr as u32, len)
                }
                Err(_) => 0,
            }
        }
    };
}
