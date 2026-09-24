use adw::prelude::*;
use crate::ExcelCsvRecord;
use crate::simple_process_password;
use crate::append_with_smart_scroll;
use std::cell::RefCell;
use std::rc::Rc;
use reqwest::Client;

pub fn create_simple_import_dialog(
    client: Client,
    token: Rc<RefCell<String>>,
    textview: gtk::TextView,
) -> adw::Dialog {
    let vbox      = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let listbox   = gtk::ListBox::new();
    
    let email_row = adw::EntryRow::builder()
        .input_purpose(gtk::InputPurpose::Email)
        .title("Account/Email")
        .build();
        
    let pass_row  = adw::PasswordEntryRow::builder()
        .input_purpose(gtk::InputPurpose::Password)
        .title("Password")
        .build();
        
    let repass_row = adw::PasswordEntryRow::builder()
        .input_purpose(gtk::InputPurpose::Password)
        .title("Confirm Password")
        .build();
        
    let change_button = gtk::Button::builder()
        .label("Change")
        .css_classes(["suggested-action"])
        .build();

    listbox.append(&email_row);
    listbox.append(&pass_row);
    listbox.append(&repass_row);
    
    let clamp = adw::Clamp::builder()
        .maximum_size(500)
        .child(&vbox)
        .build();
    vbox.append(&listbox);
    vbox.append(&change_button);

    let header_bar = adw::HeaderBar::new();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header_bar);
    toolbar_view.set_content(Some(&clamp));

    let dialog = adw::Dialog::builder()
        .can_close(true)
        .presentation_mode(adw::DialogPresentationMode::Auto)
        .title("Simple Import")
        .child(&toolbar_view)
        .content_height(400) 
        .content_width(600)
        .build();

    email_row.connect_changed(move |email_r| {
            email_r.remove_css_class("warning");
        }
    );
    
    let repass_widget = repass_row.clone().upcast::<gtk::Widget>();
    pass_row.connect_changed(glib::clone!(
        #[strong] repass_widget,
        move |pass_r| {
            pass_r.remove_css_class("warning");
            repass_widget.remove_css_class("warning");
        }
    ));
    
    let pass_widget = pass_row.clone().upcast::<gtk::Widget>();
    repass_row.connect_changed(glib::clone!(
        #[strong] pass_widget,
        move |repass_r| {
            pass_widget.remove_css_class("warning");
            repass_r.remove_css_class("warning");
        }
    ));


    change_button.connect_clicked(glib::clone!(
        #[strong] dialog,
        #[strong] textview,
        #[strong] email_row,
        #[strong] pass_row,
        #[strong] repass_row,
        #[strong] client,
        #[strong] token,
        move |button| {
            let email = email_row.text().to_string();
            let pass = pass_row.text().to_string();
            let repass = repass_row.text().to_string();
            
            if email.is_empty(){
                email_row.add_css_class("warning");
                return;
            }
            if pass != repass || pass.is_empty() {
                pass_row.add_css_class("warning");
                repass_row.add_css_class("warning");
                return;
            }
            
            pass_row.remove_css_class("warning");
            repass_row.remove_css_class("warning");

            button.set_sensitive(false);
            
            let alert_dialog = adw::AlertDialog::builder()
                .heading("بدء العملية / Start Update")
                .body("سيتم تغير كلمة المرور للحساب المدرج. لا يمكن التراجع.\nهل تريد الاستمرار؟\n\nPassword will be changed. This cannot be undone. Continue?")
                .build();
                
            alert_dialog.add_response("cancel", "إلغاء / Cancel");
            alert_dialog.add_response("start", "ابدأ / Start");
            alert_dialog.set_response_appearance("start", adw::ResponseAppearance::Destructive);
            
            let client_run = client.clone();
            let token_run = Rc::clone(&token);
            let textview_run = textview.clone();
            let dialog_run = dialog.clone();
            let button_run = button.clone();
            
            let record = ExcelCsvRecord {
                email: email,
                new_password: pass,
            };
            
            alert_dialog.choose(Some(&dialog), None::<&gtk::gio::Cancellable>, move |choice| {
                if choice == "start" {
                    dialog_run.close(); 
                    glib::spawn_future_local(async move {
                        simple_process_password(client_run, token_run, record, move |result_msg| {
                            if let Some(msg) = result_msg {
                                append_with_smart_scroll(&textview_run, &msg);
                            }
                        }).await;
                    });
                }
                button_run.set_sensitive(true);
            });
        }
    ));
    
    dialog
}
