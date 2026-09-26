use keyring::Entry;
use soup;
use soup::prelude::SessionExt;
use soup::{Message, Session};
use serde_json;
use crate::TENANT;
use crate::CLIENT_ID;
use crate::TokenResponse;

pub fn save_refresh_token(refresh_token: &str) -> Result<(), keyring::Error> {
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


pub fn load_refresh_token() -> Option<String> {
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

pub fn clear_refresh_token() {
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
}


pub async fn refresh_ms_access_token(refresh_token: &str) -> Option<String> {
    let token_url = format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", TENANT);
    let session = Session::new();
    let message = Message::new("POST", &token_url).unwrap();
    

    let body_data = format!("client_id={}&grant_type=refresh_token&refresh_token={}",CLIENT_ID,refresh_token);
    let bytes = soup::glib::Bytes::from(body_data.as_bytes());
    message.set_request_body_from_bytes(Some("application/x-www-form-urlencoded"), Some(&bytes));
    
    
    if let Ok(bytes) = session.send_and_read_future(&message,soup::glib::Priority::default()).await {
        let json_slice: &[u8] = &bytes;
        let response: Result<TokenResponse,_> = serde_json::from_slice(json_slice) ;
        match response {
            Ok(token_res) => {
                    if let Some(new_refresh) = &token_res.refresh_token {
                        let _ = save_refresh_token(new_refresh);
                    }
                    return token_res.access_token;
                },
            Err(err) => {println!("{}",err);}
            
        }
    }
    None
}

