use adw::prelude::*;
use soup;
use soup::prelude::SessionExt;
use soup::{Message, Session};
use serde_json;
use std::time::Duration;
use std::rc::Rc;
use std::path::PathBuf;
use crate::RefCell;
use crate::TENANT;
use crate::CLIENT_ID;
use crate::SCOPES;
use crate::DeviceCodeResponse;
use crate::TokenResponse;
use crate::ExcelCsvRecord;
use crate::GraphPayload;
use crate::PasswordProfile;
use crate::{save_refresh_token};
use crate::{get_data, FileType};
#[cfg(target_os = "linux")]
use webkit6::prelude::*;
#[cfg(target_os = "linux")]
use webkit6::WebView;


#[cfg(target_os = "linux")]
pub async fn get_access_token<F: FnOnce(Option<String>) -> () >(
    webview: WebView,
    entry_row: adw::EntryRow,
    toastoverlay: adw::ToastOverlay,
    callback: F
) {
    let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", TENANT);
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);

    let session = Session::new();
    let message = Message::new("POST", &device_code_url).unwrap();
    

    let body_data = format!("client_id={}&scope={}",CLIENT_ID,SCOPES);
    let bytes = soup::glib::Bytes::from(body_data.as_bytes());
    message.set_request_body_from_bytes(Some("application/x-www-form-urlencoded"), Some(&bytes));
    
    let device_res = session.send_and_read_future(&message,soup::glib::Priority::default()).await;
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    let json_slice: &[u8] = &device_res.unwrap();
    let device_res: Result<DeviceCodeResponse,_> = serde_json::from_slice(json_slice);
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    
    let device_res =  device_res.unwrap();
    let interval = Duration::from_secs(device_res.interval);
    entry_row.set_text(&device_res.user_code);
    let code = String::from(&device_res.user_code);
    
    let clone_toastoverlay = toastoverlay.clone();
    
    webview.connect_load_changed(move |wv, load_event| {
        if load_event == webkit6::LoadEvent::Finished {
            if let Some(uri) = wv.uri() {
                if uri.as_str().contains("login.microsoft") {
                    let js_code = format!(
                        r#"
                        var checkExist = setInterval(function() {{
                            var input = document.getElementById('otc') || document.querySelector('input[name="otc"]') || document.querySelector('input[type="tel"]');
                            if (input) {{
                                var nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
                                nativeInputValueSetter.call(input, '{}');
                                input.dispatchEvent(new Event('input', {{ bubbles: true }}));
                                
                                var nextBtn = document.getElementById('idSIButton9');
                                if (nextBtn) {{
                                    nextBtn.click();
                                }}
                                
                                clearInterval(checkExist);
                            }}
                        }}, 500);
                        "#,
                        code
                    );
                    let c_toastoverlay = clone_toastoverlay.clone();
                    wv.evaluate_javascript(
                        &js_code,
                        None,
                        None,
                        None::<&gtk::gio::Cancellable>,
                        move |_result| {
                            if let Err(_err)  = _result {
                                let toast = adw::Toast::builder()
                                    .title("فشل تحميل الصفحة / Page Load Failed.")
                                    .timeout(5)
                                    .build();
                                c_toastoverlay.add_toast(toast);
                            }
                        }
                    );
                }
            }
        }
    });
    webview.load_uri(&device_res.verification_uri);
    let toast = adw::Toast::builder()
        .custom_title(&gtk::Label::new(Some("\nإذا لم يُعبأ الكود تلقائياً، انسخه والصقه في المربع.\nIf code doesn't auto-fill, copy and paste it.\n")))
        .timeout(10)
        .build();
    toastoverlay.add_toast(toast);

    loop {
        glib::timeout_future(interval).await;
        let session = Session::new();
        let message = Message::new("POST", &token_url).unwrap();
        let body_data = format!("grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id={}&device_code={}",CLIENT_ID,&device_res.device_code);
        let bytes = soup::glib::Bytes::from(body_data.as_bytes());
        message.set_request_body_from_bytes(Some("application/x-www-form-urlencoded"), Some(&bytes));

        let token_res  = session.send_and_read_future(&message,soup::glib::Priority::default()).await;
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let json_slice: &[u8] = &token_res.unwrap();
        let token_res: Result<TokenResponse,_> = serde_json::from_slice(json_slice);
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let token_res = token_res.unwrap();
        
        if let Some(access_token) = token_res.access_token {
            if let Some(refresh_token) = token_res.refresh_token {
                match save_refresh_token(&refresh_token) {
                    Ok(_) => {
                        let toast = adw::Toast::builder().title("✅ تم حفظ الجلسة بنجاح / Session saved successfully").timeout(3).build();
                        toastoverlay.add_toast(toast);
                    },
                    Err(e) => {
                        let toast = adw::Toast::builder().title(&format!("❌ فشل حفظ الجلسة / Failed to save session: {}", e)).timeout(10).build();
                        toastoverlay.add_toast(toast);
                    }
                }
            } else {
                let toast = adw::Toast::builder().title("⚠️ لم يتم استلام رمز تحديث / No refresh token received").timeout(10).build();
                toastoverlay.add_toast(toast);
            }
            callback(Some(access_token));
            return ;
        } else if let Some(error) = token_res.error {
            if error == "authorization_pending" {
                continue;
            } else {
                callback(None);
                return;
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub async fn get_access_token<F: FnOnce(Option<String>) -> () >(
    open_browser_btn: gtk::Button,
    copy_code_btn: gtk::Button,
    entry_row: adw::EntryRow,
    toastoverlay: adw::ToastOverlay,
    callback: F
) {
    let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", TENANT);
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let session = Session::new();
    let message = Message::new("POST", &device_code_url).unwrap();
    

    let body_data = format!("client_id={}&scope={}",CLIENT_ID,SCOPES);
    let bytes = soup::glib::Bytes::from(body_data.as_bytes());
    message.set_request_body_from_bytes(Some("application/x-www-form-urlencoded"), Some(&bytes));
    
    let device_res = session.send_and_read_future(&message,soup::glib::Priority::default()).await;
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    let json_slice: &[u8] = &device_res.unwrap();
    let device_res: Result<DeviceCodeResponse,_> = serde_json::from_slice(json_slice);
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    
    let device_res =  device_res.unwrap();
    let interval = Duration::from_secs(device_res.interval);
    
    entry_row.set_text(&device_res.user_code);
    let code = String::from(&device_res.user_code);
    let verification_uri = String::from(&device_res.verification_uri);

    copy_code_btn.connect_clicked(move |btn| {
        let clipboard = btn.clipboard(); 
        clipboard.set_text(&code);
        btn.set_label("تم النسخ! / Copied! ✔");
    });

    open_browser_btn.connect_clicked(move |_| {
            if let Err(e) = open::that(&verification_uri) {
                eprintln!("Failed to open browser: {}", e);
            }
        });

    let toast = adw::Toast::builder()
        .custom_title(&gtk::Label::new(Some("\nانسخ الكود وافتح المتصفح للمصادقة.\nCopy code and open browser to authenticate.\n")))
        .timeout(10)
        .build();
    toastoverlay.add_toast(toast);

    loop {
        glib::timeout_future(interval).await;
        let session = Session::new();
        let message = Message::new("POST", &token_url).unwrap();
        let body_data = format!("grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id={}&device_code={}",CLIENT_ID,&device_res.device_code);
        let bytes = soup::glib::Bytes::from(body_data.as_bytes());
        message.set_request_body_from_bytes(Some("application/x-www-form-urlencoded"), Some(&bytes));

        let token_res  = session.send_and_read_future(&message,soup::glib::Priority::default()).await;
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let json_slice: &[u8] = &token_res.unwrap();
        let token_res: Result<TokenResponse,_> = serde_json::from_slice(json_slice);
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let token_res = token_res.unwrap();
        
        if let Some(access_token) = token_res.access_token {
            if let Some(refresh_token) = token_res.refresh_token {
                match save_refresh_token(&refresh_token) {
                    Ok(_) => {
                        let toast = adw::Toast::builder().title("✅ تم حفظ الجلسة بنجاح / Session saved successfully").timeout(3).build();
                        toastoverlay.add_toast(toast);
                    },
                    Err(e) => {
                        let toast = adw::Toast::builder().title(&format!("❌ فشل حفظ الجلسة / Failed to save session: {}", e)).timeout(10).build();
                        toastoverlay.add_toast(toast);
                    }
                }
            } else {
                let toast = adw::Toast::builder().title("⚠️ لم يتم استلام رمز تحديث / No refresh token received").timeout(10).build();
                toastoverlay.add_toast(toast);
            }
            callback(Some(access_token));
            return ;
        } else if let Some(error) = token_res.error {
            if error == "authorization_pending" {
                continue;
            } else {
                callback(None);
                return;
            }
        }
    }
}

pub async fn process_passwords<F: Fn(Option<String>) -> ()>(
    c_token: Rc<RefCell<String>>, 
    file_path: PathBuf,
    callback: F
) {
    let data = get_data(&file_path,FileType::Auto);
    if let Err(_e) = data {
        println!("{}",_e);
        callback(None);
        return ;
    }
    let data = data.unwrap();
    let token  = c_token.borrow().clone();  
    let mut success_count = 0;
    let mut failed_count = 0;
    callback(format!("\nبدء التحديث... / Starting update...\n").into());
    for (index, info) in data.into_iter().enumerate() {
        if let Some(email) = info.email && let Some(newpassword) = info.password {
            let current_row = index + 1;
            
            let record = ExcelCsvRecord {
                email: email,
                new_password: newpassword,
            };

            let email = record.email.trim();
            let endpoint = format!("https://graph.microsoft.com/v1.0/users/{}", email);
            
            let session = Session::new();
            let message = Message::new("PATCH", &endpoint).unwrap();
            let headers = message.request_headers().unwrap();
            headers.append("Authorization", &format!("Bearer {}", &token));
            
            let payload = GraphPayload {
                password_profile: PasswordProfile {
                    password: record.new_password.trim().to_string(),
                    force_change: false,
                },
            };
            let json_string = serde_json::to_string(&payload).unwrap();
            let bytes = soup::glib::Bytes::from(json_string.as_bytes());
            message.set_request_body_from_bytes(Some("application/json"), Some(&bytes));
            let response = session.send_and_read_future(&message,soup::glib::Priority::default()).await;
            if let Err(_e) = response {
                println!("{}",_e);
                callback(None);
                return ;
            }
            let response = response.unwrap();
            let status = message.status_code();
            let is_success = status >= 200 && status < 300;

            if is_success {
                callback(format!("✅ نجاح / Success ({}): {}\n", current_row, email).into());
                success_count += 1;
            } else {
                let json_slice: &[u8] = &response;
                match serde_json::from_slice::<serde_json::Value>(json_slice) {
                    Ok(error_json) => {
                        let error_msg = error_json["error"]["message"]
                            .as_str()
                            .unwrap_or("خطأ غير معروف / Unknown error");
                            
                        callback(format!("❌ فشل / Failed ({}): {} - السبب/Reason: {}\n", current_row, email, error_msg).into());
                        failed_count += 1;
                    }
                    Err(_e) => {
                        println!("{}", _e);
                        callback(None);
                        return;
                        
                    }
                }
            }
        }
        
        glib::timeout_future(Duration::from_millis(1000)).await;
    }

    callback(format!("\n🎯 انتهت العملية / Process finished. النجاح/Success: {}، الفشل/Failed: {}", success_count, failed_count).into());
}

pub async fn simple_process_password<F: Fn(Option<String>) -> ()>(
    c_token: Rc<RefCell<String>>, 
    simplerecord: ExcelCsvRecord,
    callback: F
) {
    let token  = c_token.borrow().clone();
    let mut success_count = 0;
    let mut failed_count = 0;
    let current_row = 1;
    callback(format!("\nبدء التحديث... / Starting update...\n").into());

    let email = simplerecord.email.trim();
    let endpoint = format!("https://graph.microsoft.com/v1.0/users/{}", email);
    let session = Session::new();
    let message = Message::new("PATCH", &endpoint).unwrap();
    let headers = message.request_headers().unwrap();
    headers.append("Authorization", &format!("Bearer {}", &token));
            
    let payload = GraphPayload {
        password_profile: PasswordProfile {
            password: simplerecord.new_password.trim().to_string(),
            force_change: false,
        },
    };

    let json_string = serde_json::to_string(&payload).unwrap();
    let bytes = soup::glib::Bytes::from(json_string.as_bytes());
    message.set_request_body_from_bytes(Some("application/json"), Some(&bytes));
    let response = session.send_and_read_future(&message,soup::glib::Priority::default()).await;

    if let Err(_e) = response {
        println!("{}",_e);
        callback(None);
        return ;
    }
    let response = response.unwrap();
    let status = message.status_code();
    let is_success = status >= 200 && status < 300;

    if is_success {
        callback(format!("✅ نجاح / Success ({}): {}\n", current_row, email).into());
        success_count += 1;
    } else {
        let json_slice: &[u8] = &response;
        match serde_json::from_slice::<serde_json::Value>(json_slice) {
            Ok(error_json) => {
                let error_msg = error_json["error"]["message"]
                    .as_str()
                    .unwrap_or("خطأ غير معروف / Unknown error");
                    
                callback(format!("❌ فشل / Failed ({}): {} - السبب/Reason: {}\n", current_row, email, error_msg).into());
                failed_count += 1;
            }
            Err(_e) => {
                println!("{}", _e);
                callback(None);
                return;
                
            }
        }
    }
    callback(format!("\n🎯 انتهت العملية / Process finished. النجاح/Success: {}، الفشل/Failed: {}", success_count, failed_count).into());
}
