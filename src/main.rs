#![windows_subsystem = "windows"]
mod utils;
mod gui;
use gui::simple_import;
use utils::read_excel_csv::{get_data, FileType};
use utils::keyring_fnc::{save_refresh_token, load_refresh_token,clear_refresh_token,refresh_ms_access_token};
use utils::change_pass_func::{simple_process_password, process_passwords,get_access_token};
use serde::{Deserialize, Serialize};
use adw::prelude::*;
use gtk::glib;
use std::path::PathBuf;
use std::env;
use std::fs;
use std::cell::RefCell;
use std::rc::Rc;


#[cfg(target_os = "windows")]
use open;


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

pub fn append_with_smart_scroll(
    text_view: &gtk::TextView,
    text: &str,
) {
    let buffer = text_view.buffer();

    let text = text.replace('\0', "");

    let mut iter = buffer.end_iter();
    buffer.insert(&mut iter, &text);

    let  end_iter = buffer.end_iter();
    buffer.place_cursor(&end_iter);

    if let Some(mark) = buffer.mark("insert") {
        text_view.scroll_to_mark(
            &mark,
            0.0,
            true,
            0.0,
            1.0,
        );
    }
}



fn main() {    
    let app = adw::Application::builder().application_id("com.github.yucefsourani.schoolpasssync").build();
    app.connect_activate(|app| {
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

        

        let token_for_simple = Rc::clone(&token);
        let textview_for_simple = textview.clone();
        simple_import_button.connect_clicked(glib::clone!(
            #[strong] mainwindow,
            move |_| {
                let dialog = simple_import::create_simple_import_dialog(
                    token_for_simple.clone(), 
                    textview_for_simple.clone()
                );
                dialog.present(Some(&mainwindow));
            }
        ));

        let c1_token = Rc::clone(&token);
        get_file_path_button.connect_clicked(glib::clone!(
            #[strong] file_dialog,
            #[weak] mainwindow,
            #[strong] textview,
            move |_| {
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
                            
                            let token_run = Rc::clone(&token_inner);
                            let textview_run = c_textview.clone();
                            let path_run = path.clone();

                            dialog.choose(Some(&mainwindow_inner), None::<&gtk::gio::Cancellable>, move |choice| {
                                if choice == "start" {
                                    glib::spawn_future_local(async move {
                                        process_passwords(token_run, path_run, move |result_msg| {
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
                            get_access_token(webview_in, entry_row_in, toastoverlay_in, move |msg| { auth_cb_in(msg); }).await;
                        });
                    }
                    #[cfg(target_os = "windows")]
                    {
                        main_stack_in.set_visible_child_name("windows_auth");
                        glib::spawn_future_local(async move {
                            get_access_token(open_btn_in, copy_btn_in, entry_row_in, toastoverlay_in, move |msg| { auth_cb_in(msg); }).await;
                        });
                    }
                }
            });
        });

        main_stack.set_visible_child_name("loading");
        
        let on_auth_init = Rc::clone(&on_auth_success);
        
        glib::spawn_future_local(glib::clone!(
            #[strong] main_stack,
            async move {
                let mut requires_login = true;
                
                if let Some(saved_rt) = load_refresh_token() {
                    if let Some(new_at) = refresh_ms_access_token( &saved_rt).await {
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
                        get_access_token( webview, entry_row, toastoverlay, move |msg| { on_auth_init(msg); }).await;
                    }
                    #[cfg(target_os = "windows")]
                    {
                        main_stack.set_visible_child_name("windows_auth");
                        get_access_token(open_browser_btn, copy_code_btn, entry_row, toastoverlay, move |msg| { on_auth_init(msg); }).await;
                    }
                }
            }
        ));

        mainwindow.present();
    });
    app.run();
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
