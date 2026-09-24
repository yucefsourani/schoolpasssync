#![windows_subsystem = "windows"]
mod utils;
mod gui;
use gui::simple_import;
use utils::read_excel_csv::{get_data, FileType};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use adw::prelude::*;
use gtk::glib;
use std::path::PathBuf;
use std::env;
use std::fs;
use std::cell::RefCell;
use std::rc::Rc;
use keyring::Entry;

#[cfg(target_os = "windows")]
use open;

#[cfg(target_os = "linux")]
use webkit6::prelude::*;
#[cfg(target_os = "linux")]
use webkit6::WebView;

const CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";
const TENANT: &str = "organizations";
const SCOPES: &str = "User.ReadWrite.All offline_access";
const VERSION: &str = "0.1.2";

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
    refresh_token: Option<String>,
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

pub fn append_with_smart_scroll(text_view: &gtk::TextView, text: &str) {
    let buffer = text_view.buffer();
    let mut iter = buffer.end_iter();
    buffer.insert(&mut iter, text);

    let tv = text_view.clone();
    glib::idle_add_local(move || {
        let buffer = tv.buffer();
        let mut iter = buffer.end_iter();
        buffer.place_cursor(&iter);
        
        if let Some(mark) = buffer.mark("insert") {
            tv.scroll_to_mark(&mark, 0.0, true, 0.0, 1.0);
        }
        glib::ControlFlow::Break
    });
}

fn save_refresh_token(refresh_token: &str) -> Result<(), keyring::Error> {
    clear_refresh_token();

    let chunk_size = 1000;
    let bytes = refresh_token.as_bytes();
    let chunks: Vec<&[u8]> = bytes.chunks(chunk_size).collect();

    let count_entry = Entry::new("SchoolPassSync", "ms_refresh_token_count")?;
    count_entry.set_password(&chunks.len().to_string())?;

    for (i, chunk) in chunks.iter().enumerate() {
        let entry = Entry::new("SchoolPassSync", &format!("ms_refresh_token_part_{}", i))?;
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        entry.set_password(chunk_str)?;
    }

    Ok(())
}

fn load_refresh_token() -> Option<String> {
    let count_entry = Entry::new("SchoolPassSync", "ms_refresh_token_count").ok()?;
    let count_str = count_entry.get_password().ok()?;
    let count: usize = count_str.parse().unwrap_or(0);

    if count == 0 {
        return None;
    }

    let mut full_token = String::new();
    for i in 0..count {
        let entry = Entry::new("SchoolPassSync", &format!("ms_refresh_token_part_{}", i)).ok()?;
        full_token.push_str(&entry.get_password().ok()?);
    }

    Some(full_token)
}

fn clear_refresh_token() {
    if let Ok(count_entry) = Entry::new("SchoolPassSync", "ms_refresh_token_count") {
        if let Ok(count_str) = count_entry.get_password() {
            if let Ok(count) = count_str.parse::<usize>() {
                for i in 0..count {
                    if let Ok(entry) = Entry::new("SchoolPassSync", &format!("ms_refresh_token_part_{}", i)) {
                        let _ = entry.delete_password();
                    }
                }
            }
        }
        let _ = count_entry.delete_password();
    }
    
    if let Ok(old_entry) = Entry::new("SchoolPassSync", "ms_refresh_token") {
        let _ = old_entry.delete_password();
    }
}

async fn refresh_ms_access_token(client: Client, refresh_token: &str) -> Option<String> {
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let params = [
        ("client_id", CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];

    let response = client.post(&token_url).form(&params).send().await;
    if let Ok(res) = response {
        if let Ok(token_res) = res.json::<TokenResponse>().await {
            if let Some(new_refresh) = &token_res.refresh_token {
                let _ = save_refresh_token(new_refresh);
            }
            return token_res.access_token;
        }
    }
    None
}

fn main() {
    let tokio_rt = tokio::runtime::Runtime::new().expect("Tokio Runtime Faild.");
    let _guard = tokio_rt.enter();
    
    let app = adw::Application::builder().application_id("com.github.yucefsourani.schoolpasssync").build();
    app.connect_activate(|app| {
        let client = Client::new();
        let token = Rc::new(RefCell::new(String::new()));
        
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
        about_dialog.set_developers(&["yucef mouhammad nazih sourani"]);
        
        let top_about_button = gtk::Button::from_icon_name("help-about-symbolic");
        top_about_button.connect_clicked(glib::clone!(
            #[strong] about_dialog,
            #[weak] mainwindow,
            move |_| { about_dialog.present(Some(&mainwindow)); }
        ));

        let top_logout_button = gtk::Button::from_icon_name("system-log-out-symbolic");
            
        headerbar.pack_end(&top_about_button);
        headerbar.pack_end(&top_logout_button);
        
        let main_stack = adw::ViewStack::new();
        toolbarview.set_content(Some(&main_stack));

        let loading_vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        loading_vbox.set_valign(gtk::Align::Center);
        loading_vbox.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::builder().spinning(true).width_request(50).height_request(50).build();
        loading_vbox.append(&spinner);
        let loading_label = gtk::Label::new(Some("جاري التحقق من الجلسة... / Verifying session..."));
        loading_vbox.append(&loading_label);
        main_stack.add_named(&loading_vbox, Some("loading"));
        
        let mainvbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let clamp = adw::Clamp::builder().maximum_size(600).child(&mainvbox).build();
        
        let button_vbox = gtk::Box::new(gtk::Orientation::Vertical,10);
        let get_file_path_button = gtk::Button::builder()
            .label("Open")
            .css_classes(["suggested-action"])
            .width_request(200)
            .height_request(50)
            .hexpand(false)
            .vexpand(false)
            .halign(gtk::Align::Center)
            .build();
        button_vbox.append(&get_file_path_button);
                                                
        let list_filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        let doc_filter = gtk::FileFilter::new();
        doc_filter.set_name(Some("Documents"));
        doc_filter.add_mime_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
        doc_filter.add_mime_type("application/vnd.ms-excel.sheet.macroEnabled.12");
        doc_filter.add_mime_type("application/vnd.ms-excel.sheet.binary.macroEnabled.12");
        doc_filter.add_mime_type("application/vnd.ms-excel");
        doc_filter.add_mime_type("text/csv");
        doc_filter.add_mime_type("application/vnd.ms-excel.addin.macroEnabled.12");
        doc_filter.add_mime_type("application/vnd.oasis.opendocument.spreadsheet");
        doc_filter.add_mime_type("text/x-csv");
        doc_filter.add_mime_type("application/csv");
        doc_filter.add_mime_type("text/comma-separated-values");
        doc_filter.add_mime_type("text/x-comma-separated-values");
        list_filters.append(&doc_filter);
        
        let file_dialog = gtk::FileDialog::builder().filters(&list_filters).modal(true).build();

        let statuspage = adw::StatusPage::builder()
            .title("فتح ملف / Open File")
            .description(".xlsx,.xls,.xlsm,csv...")
            .child(&button_vbox)
            .icon_name("document-open-symbolic")
            .build();
        
        let simple_import_button = gtk::Button::builder()
            .label("Simple/حساب واحد")
            .css_classes(["suggested-action"])
            .width_request(200)
            .height_request(50)
            .hexpand(false)
            .vexpand(false)
            .halign(gtk::Align::Center)
            .build();
        button_vbox.append(&simple_import_button);
        
        mainvbox.append(&statuspage);
        let sw = gtk::ScrolledWindow::new();
        mainvbox.append(&sw);
        
        let textview = gtk::TextView::new();
        textview.set_hexpand(true);
        textview.set_vexpand(true);
        textview.set_editable(false);
        textview.set_direction(gtk::TextDirection::Ltr);
        textview.set_wrap_mode(gtk::WrapMode::Word);
        textview.set_margin_bottom(10);
        sw.set_child(Some(&textview));


        let client_run = client.clone();
        let token_run = Rc::clone(&token);
        let textview_run = textview.clone();
        
        let client_for_simple = client.clone();
        let token_for_simple = Rc::clone(&token);
        let textview_for_simple = textview.clone();
        simple_import_button.connect_clicked(glib::clone!(
            #[strong] mainwindow,
            move |_| {
                let dialog = simple_import::create_simple_import_dialog(
                    client_for_simple.clone(), 
                    token_for_simple.clone(), 
                    textview_for_simple.clone()
                );
                dialog.present(Some(&mainwindow));
            }
        ));

        let c1_client = client.clone();
        let c1_token = Rc::clone(&token);
        get_file_path_button.connect_clicked(glib::clone!(
            #[strong] file_dialog,
            #[weak] mainwindow,
            #[strong] textview,
            move |_| {
                let client_inner = c1_client.clone();
                let token_inner = Rc::clone(&c1_token);
                let c_textview = textview.clone();
                let mainwindow_inner = mainwindow.clone();
                
                file_dialog.open(Some(&mainwindow), None::<&gtk::gio::Cancellable>, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            let dialog = adw::AlertDialog::builder()
                                .heading("بدء العملية / Start Update")
                                .body("سيتم تغيير كلمات المرور للحسابات المدرجة. لا يمكن التراجع.\nهل تريد الاستمرار؟\n\nPasswords will be changed for listed accounts. This cannot be undone. Continue?")
                                .build();
                            dialog.add_response("cancel", "إلغاء / Cancel");
                            dialog.add_response("start", "ابدأ / Start");
                            dialog.set_response_appearance("start", adw::ResponseAppearance::Destructive);
                            
                            let client_run = client_inner.clone();
                            let token_run = Rc::clone(&token_inner);
                            let textview_run = c_textview.clone();
                            let path_run = path.clone();

                            dialog.choose(Some(&mainwindow_inner), None::<&gtk::gio::Cancellable>, move |choice| {
                                if choice == "start" {
                                    glib::spawn_future_local(async move {
                                        process_passwords(client_run, token_run, path_run, move |result_msg| {
                                            if let Some(msg) = result_msg {
                                                append_with_smart_scroll(&textview_run, &msg);
                                            }
                                        }).await;
                                    });
                                }
                            });
                        }
                    }
                });
            }
        ));
        
        let entry_row = adw::EntryRow::builder().show_apply_button(false).margin_top(5).margin_bottom(5).margin_start(5).margin_end(5).editable(false).build();
        let listbox_entry_row = gtk::ListBox::new();
        listbox_entry_row.append(&entry_row);
        main_stack.add_named(&clamp, Some("mainvbox"));

        #[cfg(target_os = "linux")]
        let webview = WebView::new();

        #[cfg(target_os = "linux")]
        {
            let webview_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
            webview.set_hexpand(true);
            webview.set_vexpand(true);
            webview_vbox.append(&listbox_entry_row);
            webview_vbox.append(&webview);
            main_stack.add_named(&webview_vbox, Some("webview"));
        }

        #[cfg(target_os = "windows")]
        let open_browser_btn = gtk::Button::builder().label("2. فتح المتصفح / Open Browser").margin_top(10).css_classes(["pill"]).build();
        #[cfg(target_os = "windows")]
        let copy_code_btn = gtk::Button::builder().label("1. نسخ الكود / Copy Code").margin_top(10).css_classes(["pill"]).build();

        #[cfg(target_os = "windows")]
        {
            let windows_auth_vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
            windows_auth_vbox.set_valign(gtk::Align::Center);
            windows_auth_vbox.set_halign(gtk::Align::Center);
            let label = gtk::Label::new(Some("انسخ الكود وافتح المتصفح لتسجيل الدخول:\nCopy the code and open the browser to login:"));
            windows_auth_vbox.append(&label);
            windows_auth_vbox.append(&listbox_entry_row);
            windows_auth_vbox.append(&copy_code_btn);
            windows_auth_vbox.append(&open_browser_btn);
            main_stack.add_named(&windows_auth_vbox, Some("windows_auth"));
        }

        let c2_token = Rc::clone(&token);
        let main_stack_auth = main_stack.clone();
        let on_auth_success: Rc<dyn Fn(Option<String>)> = Rc::new(move |access_token: Option<String>| {
            if let Some(at) = access_token {
                *c2_token.borrow_mut() = at;
                main_stack_auth.set_visible_child_name("mainvbox");
            }
        });

        let mainwindow_logout = mainwindow.clone();
        let main_stack_logout = main_stack.clone();
        let token_logout = Rc::clone(&token);
        

        let client_logout = client.clone();
        
        let entry_row_logout = entry_row.clone();
        let toastoverlay_logout = toastoverlay.clone();
        #[cfg(target_os = "linux")]
        let webview_logout = webview.clone();
        #[cfg(target_os = "windows")]
        let open_browser_btn_logout = open_browser_btn.clone();
        #[cfg(target_os = "windows")]
        let copy_code_btn_logout = copy_code_btn.clone();
        let on_auth_success_logout = Rc::clone(&on_auth_success);

        top_logout_button.connect_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading("تسجيل الخروج / Logout")
                .body("هل تريد تسجيل الخروج وحذف بيانات الجلسة؟\n\nAre you sure you want to logout and delete session data?")
                .build();
            dialog.add_response("cancel", "إلغاء / Cancel");
            dialog.add_response("logout", "خروج / Logout");
            dialog.set_response_appearance("logout", adw::ResponseAppearance::Destructive);
            
            let client_in = client_logout.clone();
            let token_in = Rc::clone(&token_logout);
            let main_stack_in = main_stack_logout.clone();
            let entry_row_in = entry_row_logout.clone();
            let toastoverlay_in = toastoverlay_logout.clone();
            #[cfg(target_os = "linux")]
            let webview_in = webview_logout.clone();
            #[cfg(target_os = "windows")]
            let open_btn_in = open_browser_btn_logout.clone();
            #[cfg(target_os = "windows")]
            let copy_btn_in = copy_code_btn_logout.clone();
            let auth_cb_in = Rc::clone(&on_auth_success_logout);

            dialog.choose(Some(&mainwindow_logout), None::<&gtk::gio::Cancellable>, move |choice| {
                if choice == "logout" {
                    clear_refresh_token();
                    *token_in.borrow_mut() = String::new();
                    
                    #[cfg(target_os = "linux")]
                    {
                        main_stack_in.set_visible_child_name("webview");
                        glib::spawn_future_local(async move {
                            get_access_token(client_in, webview_in, entry_row_in, toastoverlay_in, move |msg| { auth_cb_in(msg); }).await;
                        });
                    }
                    #[cfg(target_os = "windows")]
                    {
                        main_stack_in.set_visible_child_name("windows_auth");
                        glib::spawn_future_local(async move {
                            get_access_token(client_in, open_btn_in, copy_btn_in, entry_row_in, toastoverlay_in, move |msg| { auth_cb_in(msg); }).await;
                        });
                    }
                }
            });
        });

        main_stack.set_visible_child_name("loading");
        
        let client_init = client.clone();
        let on_auth_init = Rc::clone(&on_auth_success);
        
        glib::spawn_future_local(glib::clone!(
            #[strong] main_stack,
            async move {
                let mut requires_login = true;
                
                if let Some(saved_rt) = load_refresh_token() {
                    if let Some(new_at) = refresh_ms_access_token(client_init.clone(), &saved_rt).await {
                        on_auth_init(Some(new_at));
                        requires_login = false;
                    } else {
                        clear_refresh_token();
                    }
                }

                if requires_login {
                    #[cfg(target_os = "linux")]
                    {
                        main_stack.set_visible_child_name("webview");
                        get_access_token(client_init, webview, entry_row, toastoverlay, move |msg| { on_auth_init(msg); }).await;
                    }
                    #[cfg(target_os = "windows")]
                    {
                        main_stack.set_visible_child_name("windows_auth");
                        get_access_token(client_init, open_browser_btn, copy_code_btn, entry_row, toastoverlay, move |msg| { on_auth_init(msg); }).await;
                    }
                }
            }
        ));

        mainwindow.present();
    });
    app.run();
}

#[cfg(target_os = "linux")]
async fn get_access_token<F: FnOnce(Option<String>) -> () >(
    client: Client, 
    webview: WebView,
    entry_row: adw::EntryRow,
    toastoverlay: adw::ToastOverlay,
    callback: F
) {
    let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", TENANT);
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let params = [("client_id", CLIENT_ID), ("scope", SCOPES)];

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
async fn get_access_token<F: FnOnce(Option<String>) -> () >(
    client: Client, 
    open_browser_btn: gtk::Button,
    copy_code_btn: gtk::Button,
    entry_row: adw::EntryRow,
    toastoverlay: adw::ToastOverlay,
    callback: F
) {
    let device_code_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode", TENANT);
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let params = [("client_id", CLIENT_ID), ("scope", SCOPES)];

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

async fn process_passwords<F: Fn(Option<String>) -> ()>(
    client: Client, 
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
                callback(format!("✅ نجاح / Success ({}): {}\n", current_row, email).into());
                success_count += 1;
            } else {
                let error_json = response.json().await;
                if let Err(_e) = error_json {
                    println!("{}",_e);
                    callback(None);
                    return ;
                }
                let error_json: serde_json::Value = error_json.unwrap();
                
                let error_msg = error_json["error"]["message"].as_str().unwrap_or("خطأ غير معروف / Unknown error");
                callback(format!("❌ فشل / Failed ({}): {} - السبب/Reason: {}\n", current_row, email, error_msg).into());
                failed_count += 1;
            }
        }

        sleep(Duration::from_millis(500)).await;
    }

    callback(format!("\n🎯 انتهت العملية / Process finished. النجاح/Success: {}، الفشل/Failed: {}", success_count, failed_count).into());
}

async fn simple_process_password<F: Fn(Option<String>) -> ()>(
    client: Client, 
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

    let payload = GraphPayload {
        password_profile: PasswordProfile {
            password: simplerecord.new_password.trim().to_string(),
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
        callback(format!("✅ نجاح / Success ({}): {}\n", current_row, email).into());
        success_count += 1;
    } else {
        let error_json = response.json().await;
        if let Err(_e) = error_json {
            println!("{}",_e);
            callback(None);
            return ;
        }
        let error_json: serde_json::Value = error_json.unwrap();
        
        let error_msg = error_json["error"]["message"].as_str().unwrap_or("خطأ غير معروف / Unknown error");
        callback(format!("❌ فشل / Failed ({}): {} - السبب/Reason: {}\n", current_row, email, error_msg).into());
        failed_count += 1;
    }
    callback(format!("\n🎯 انتهت العملية / Process finished. النجاح/Success: {}، الفشل/Failed: {}", success_count, failed_count).into());
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
