mod utils;
use utils::read_excel_csv::{get_data,FileType};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use adw::prelude::*;
use gtk::glib;
use futures::channel;
use std::path::PathBuf;
use std::env;
use std::fs;
use std::cell::RefCell;
use std::rc::Rc;


#[cfg(target_os = "linux")]
use webkit6::prelude::*;
#[cfg(target_os = "linux")]
use webkit6::WebView;

const CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";
const TENANT: &str = "organizations";
const SCOPES: &str = "User.ReadWrite.All offline_access";
const VERSION: &str = "0.1.0";

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    message: String,
    interval: u64,
}

#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ExcelCsvRecord {
    email: String,
    new_password: String,
}

#[derive(Serialize)]
struct PasswordProfile {
    password: String,
    #[serde(rename = "forceChangePasswordNextSignIn")]
    force_change: bool,
}

#[derive(Serialize)]
struct GraphPayload {
    #[serde(rename = "passwordProfile")]
    password_profile: PasswordProfile,
}

fn main() {
    let tokio_rt = tokio::runtime::Runtime::new().expect("Tokio Runtime Faild.");
    let _guard = tokio_rt.enter();
    
    let app = adw::Application::builder().application_id("com.github.yucefsourani.schoolpasssync").build();
    app.connect_activate(|app| {
        let client = Rc::new(RefCell::new(Client::new()));
        let  token =  Rc::new(RefCell::new(String::new()));
        let mainwindow = adw::ApplicationWindow::builder().application(app).title("SchoolPassSync").build();
        mainwindow.maximize();

        if let Some(exe_dir) = get_exe_dir() {
            if let Some(display) = gtk::gdk::Display::default() {
                let icon_theme = gtk::IconTheme::for_display(&display);
                icon_theme.add_search_path(exe_dir.as_path());
            }
        }
        
        let toastoverlay = adw::ToastOverlay::new();
        mainwindow.set_content(Some(&toastoverlay));

        let toolbarview = adw::ToolbarView::new();
        toastoverlay.set_child(Some(&toolbarview));
        
        let headerbar = gtk::HeaderBar::new();
        toolbarview.add_top_bar(&headerbar);
        
        let about_dialog = adw::AboutDialog::new();
        about_dialog.set_application_icon("com.github.yucefsourani.schoolpasssync");
        about_dialog.set_application_name("SchoolPassSync");
        about_dialog.set_copyright("© 2026 Yucef");
        about_dialog.set_developer_name("Yucef Sourani");
        about_dialog.set_license_type(gtk::License::Gpl30);
        about_dialog.set_version(VERSION);
        about_dialog.set_website("https://github.com/yucefsourani/schoolpasssync");
        about_dialog.set_support_url("https://github.com/yucefsourani/schoolpasssync");
        about_dialog.set_developers(&["yucef mouhammad nazih  sourani"]);
        
        let top_about_button = gtk::Button::from_icon_name("help-about-symbolic");
        top_about_button.connect_clicked(glib::clone!(
            #[strong] about_dialog,
            #[weak] mainwindow,
            move |_| {
                about_dialog.present(Some(&mainwindow));
            }));
            
        headerbar.pack_end(&top_about_button);
        
        let main_stack = adw::ViewStack::new();
        toolbarview.set_content(Some(&main_stack));

        let mainvbox     = gtk::Box::new(gtk::Orientation::Vertical, 10);
        
        let clamp = adw::Clamp::builder()
                              .maximum_size(600)
                              .child(&mainvbox)
                              .build();
        
        let get_file_path_button = gtk::Button::builder()
                                                .label("Open")
                                                .css_classes(["suggested-action"])
                                                .width_request(200)
                                                .height_request(50)
                                                .hexpand(false)
                                                .vexpand(false)
                                                .halign(gtk::Align::Center)
                                                .build();
                                                
        let list_filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        let doc_filter    = gtk::FileFilter::new();
        doc_filter.set_name(Some("Documents"));
        doc_filter.add_mime_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        doc_filter.add_mime_type("application/vnd.ms-excel.sheet.macroEnabled.12");
        doc_filter.add_mime_type("application/vnd.ms-excel.sheet.binary.macroEnabled.12");
        doc_filter.add_mime_type("application/vnd.ms-excel");
        doc_filter.add_mime_type("application/vnd.ms-excel.addin.macroEnabled.12");
        doc_filter.add_mime_type("application/vnd.oasis.opendocument.spreadsheet");
        doc_filter.add_mime_type("text/csv");
        doc_filter.add_mime_type("text/x-csv");
        doc_filter.add_mime_type("application/csv");
        doc_filter.add_mime_type("text/comma-separated-values");
        doc_filter.add_mime_type("text/x-comma-separated-values");
        list_filters.append(&doc_filter);
        
        let file_dialog = gtk::FileDialog::builder()
                                  .filters(&list_filters)
                                  .modal(true)
                                  .build();

        let statuspage = adw::StatusPage::builder()
                                        .title("Open Excel File")
                                        .description(".xlsx,.xls,.xlsm,csv...")
                                        .child(&get_file_path_button)
                                        .icon_name("document-open-symbolic")
                                        .build();
        mainvbox.append(&statuspage);
        
        let sw = gtk::ScrolledWindow::new();
        mainvbox.append(&sw);
        
        let textview = gtk::TextView::new();
        textview.set_hexpand(true);
        textview.set_vexpand(true);
        textview.set_editable(false);
        textview.set_direction(gtk::TextDirection::Ltr);
        sw.set_child(Some(&textview));
        let textbuffer = textview.buffer();
        
        let c1_client = Rc::clone(&client);
        let c1_token  = Rc::clone(&token);
        get_file_path_button.connect_clicked(glib::clone!(
            #[strong] file_dialog,
            #[weak] mainwindow,
            #[strong] textbuffer,
            move |_| {
                let client = Rc::clone(&c1_client);
                let token = Rc::clone(&c1_token);
                let c_textbuffer = textbuffer.clone();
                file_dialog.open(Some(&mainwindow),None::<&gtk::gio::Cancellable>,move |result|{
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            glib::spawn_future_local(async move {
                                process_passwords(client,token,path,|result|{
                                    if let Some(msg) = result {
                                        let mut iter = c_textbuffer.end_iter();
                                        c_textbuffer.insert(&mut iter,&msg);
                                    }
                                }).await;
                            });
                        }
                    }
                });
            }
        ));
        
        let entry_row = adw::EntryRow::builder()
                                      .show_apply_button(false)
                                      .margin_top(5)
                                      .margin_bottom(5)
                                      .margin_start(5)
                                      .margin_end(5)
                                      .editable(false)
                                      .build();
                                          
        main_stack.add_named(&clamp,Some("mainvbox"));

        #[cfg(target_os = "linux")]
        let webview = WebView::new();

        #[cfg(target_os = "linux")]
        {
            let webview_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
            webview.set_hexpand(true);
            webview.set_vexpand(true);
            webview_vbox.append(&entry_row);
            webview_vbox.append(&webview);
            main_stack.add_named(&webview_vbox,Some("webview"));
            main_stack.set_visible_child_name("webview");
        }

        #[cfg(target_os = "windows")]
        let open_browser_btn = gtk::Button::builder().label("2. فتح صفحة تسجيل الدخول").margin_top(10).css_classes(["pill"]).build();
        #[cfg(target_os = "windows")]
        let copy_code_btn = gtk::Button::builder().label("1. نسخ الكود").margin_top(10).css_classes(["pill"]).build();

        #[cfg(target_os = "windows")]
        {
            let windows_auth_vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
            windows_auth_vbox.set_valign(gtk::Align::Center);
            windows_auth_vbox.set_halign(gtk::Align::Center);
            
            let label = gtk::Label::new(Some("يرجى نسخ الكود وفتح المتصفح لتسجيل الدخول:"));
            
            windows_auth_vbox.append(&label);
            windows_auth_vbox.append(&entry_row);
            windows_auth_vbox.append(&copy_code_btn);
            windows_auth_vbox.append(&open_browser_btn);

            main_stack.add_named(&windows_auth_vbox, Some("windows_auth"));
            main_stack.set_visible_child_name("windows_auth");
        }

        let (tx, rx) = channel::oneshot::channel::<Option<String>>();
        let c2_token = Rc::clone(&token);
        
        glib::MainContext::default().spawn_local(glib::clone!(
            #[strong] main_stack,
            async move {
                if let Ok(message) = rx.await {
                    if let Some(access_token) = message {
                        let mut token = c2_token.borrow_mut();
                        *token = access_token;
                        main_stack.set_visible_child_name("mainvbox");
                    }
                }
        }));
        
        let c2_client = Rc::clone(&client);

        #[cfg(target_os = "linux")]
        {
            let webview_clone = webview.clone();
            glib::spawn_future_local(async move {
                get_access_token(
                    c2_client,
                    webview_clone,
                    entry_row,
                    toastoverlay,
                    move |msg: Option<String>| {
                        let _ = tx.send(msg);
                    }
                ).await;
            });
        }


        #[cfg(target_os = "windows")]
        {
            glib::spawn_future_local(async move {
                get_access_token(
                    c2_client,
                    open_browser_btn,
                    copy_code_btn,
                    entry_row,
                    toastoverlay,
                    move |msg: Option<String>| {
                        let _ = tx.send(msg);
                    }
                ).await;
            });
        }

        mainwindow.present();
    });
    app.run();
}



#[cfg(target_os = "linux")]
async fn get_access_token<F: FnOnce(Option<String>) -> () >(
    c2_client: Rc<RefCell<Client>>,
    webview: WebView,
    entry_row: adw::EntryRow,
    toastoverlay: adw::ToastOverlay,
    callback: F
) {
    let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", TENANT);
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let params = [("client_id", CLIENT_ID), ("scope", SCOPES)];

    let client = c2_client.borrow();
    let device_res = client.post(&device_code_url).form(&params).send().await;
        
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    let device_res = device_res.unwrap().json().await;
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    
    let device_res: DeviceCodeResponse = device_res.unwrap();
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
                                    .title("Load Page Faild.")
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
        .custom_title(&gtk::Label::new(Some("\nIf the code doesn't auto-fill,\n\ncopy it from the field above and paste it into the Code box.\n")))
        .timeout(10)
        .build();
    toastoverlay.add_toast(toast);

    loop {
        sleep(interval).await;
        let token_params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", &device_res.device_code),
        ];

        let token_res  = client.post(&token_url).form(&token_params).send().await;
        
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let token_res = token_res.unwrap().json().await;
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let token_res: TokenResponse = token_res.unwrap();
        
        if let Some(access_token) = token_res.access_token {
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
async fn get_access_token<F: FnOnce(Option<String>) -> () >(
    c2_client: Rc<RefCell<Client>>,
    open_browser_btn: gtk::Button,
    copy_code_btn: gtk::Button,
    entry_row: adw::EntryRow,
    toastoverlay: adw::ToastOverlay,
    callback: F
) {
    let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", TENANT);
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let params = [("client_id", CLIENT_ID), ("scope", SCOPES)];

    let client = c2_client.borrow();
    let device_res = client.post(&device_code_url).form(&params).send().await;
        
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    let device_res = device_res.unwrap().json().await;
    if let Err(_e) = device_res {
        callback(None);
        return;
    }
    
    let device_res: DeviceCodeResponse = device_res.unwrap();
    let interval = Duration::from_secs(device_res.interval);
    

    entry_row.set_text(&device_res.user_code);
    let code = String::from(&device_res.user_code);
    let verification_uri = String::from(&device_res.verification_uri);

    copy_code_btn.connect_clicked(move |btn| {
        let clipboard = btn.clipboard(); 
        clipboard.set_text(&code);
        btn.set_label("تم النسخ! ✔");
        
    });

    open_browser_btn.connect_clicked(move |_| {
        let _ = gtk::gio::AppInfo::launch_default_for_uri(&verification_uri, None::<&gtk::gio::AppLaunchContext>);
    });

    let toast = adw::Toast::builder()
        .custom_title(&gtk::Label::new(Some("\nانسخ الكود وافتح المتصفح للمصادقة.\n")))
        .timeout(10)
        .build();
    toastoverlay.add_toast(toast);

    loop {
        sleep(interval).await;
        let token_params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", &device_res.device_code),
        ];

        let token_res  = client.post(&token_url).form(&token_params).send().await;
        
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let token_res = token_res.unwrap().json().await;
        if let Err(_e) = token_res {
            callback(None);
            return ;
        }
        
        let token_res: TokenResponse = token_res.unwrap();
        
        if let Some(access_token) = token_res.access_token {
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

async fn process_passwords<F: Fn(Option<String>) -> ()>(c1_client: Rc<RefCell<Client>>, c_token: Rc<RefCell<String>>, file_path: PathBuf,callback: F)  {
    let data = get_data(&file_path,FileType::Auto);
    if let Err(_e) = data {
        println!("{}",_e);
        callback(None);
        return ;
    }
    let data = data.unwrap();
    let client = c1_client.borrow();
    let token  = c_token.borrow();
    let mut success_count = 0;
    let mut failed_count = 0;
    callback(format!("\nبدء عملية التحديث...\n").into());
    for (index, info) in data.into_iter().enumerate() {
        if let Some(email) = info.email && let Some(newpassword) = info.password {
            let current_row = index + 1;
            
            let record = ExcelCsvRecord {
                email: email,
                new_password: newpassword,
            };

            let email = record.email.trim();
            let endpoint = format!("https://graph.microsoft.com/v1.0/users/{}", email);

            let payload = GraphPayload {
                password_profile: PasswordProfile {
                    password: record.new_password.trim().to_string(),
                    force_change: false,
                },
            };

            let response = client.patch(&endpoint)
                .bearer_auth(&token)
                .json(&payload)
                .send()
                .await;
            if let Err(_e) = response {
                println!("{}",_e);
                callback(None);
                return ;
            }
            let response = response.unwrap();

            if response.status().is_success() {
                callback(format!("✅ نجاح ({}): {}\n", current_row, email).into());
                success_count += 1;
            } else {
                let error_json = response.json().await;
                if let Err(_e) = error_json {
                    println!("{}",_e);
                    callback(None);
                    return ;
                }
                let error_json: serde_json::Value = error_json.unwrap();
                let _error_msg = error_json["error"]["message"].as_str().unwrap_or("خطأ غير معروف");
                callback(format!("❌ فشل ({}): {}\n", current_row, email).into());
                failed_count += 1;
            }
        }

        sleep(Duration::from_millis(500)).await;
    }

    callback(format!("\n🎯 انتهت العملية. النجاح: {}، الفشل: {}", success_count, failed_count).into());
}

#[allow(dead_code)]
fn get_exe_dir() -> Option<PathBuf> {
    let mut exe_path = env::current_exe().ok()?;
    exe_path.pop();
    Some(exe_path)
}

#[allow(dead_code)]
fn is_file(path: &str,linux_symlink:bool) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        let is_file = metadata.file_type().is_file() ;
        if linux_symlink == true {
            return is_file;
        } else {
            if metadata.file_type().is_symlink() {
                return false;
            }else {
                return is_file;
            }
            
        }
    }
    false
}

#[allow(dead_code)]
fn is_dir(path: &str,linux_symlink:bool) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        let is_dir = metadata.file_type().is_dir() ;
        if linux_symlink == true {
            return is_dir;
        } else {
            if metadata.file_type().is_symlink() {
                return false;
            }else {
                return is_dir;
            }
            
        }
    }
    false
}

#[allow(dead_code)]
fn join_paths(dir: &str, file: &str) -> String {
    let mut path = PathBuf::from(dir);
    path.push(file);
    path.to_string_lossy().into_owned()
}

#[allow(dead_code)]
fn get_icon_path(icon_name:&str) -> Option<String> {
    if let Some(location)  = get_icons_location(){
        let icon_name_location = join_paths(&location,icon_name);
        if is_file(&icon_name_location,true){
            return Some(icon_name_location);
        }
        
    }
    None
}
    
#[allow(dead_code)]
fn get_icons_location() -> Option<String>  {
    if let Some(mut dir) = get_exe_dir() {
        let mut clone_dir = dir.clone();
        
        dir.push("../../images");
        let dir = dir.to_string_lossy().into_owned();
        if is_dir(&dir,true) {
            return Some(dir);
        }

        clone_dir.push("../share/schoolpasssync/images");
        let clone_dir = clone_dir.to_string_lossy().into_owned();
        if is_dir(&clone_dir,true) {
            return Some(clone_dir);
        }
        return None;
    }
    None
}
