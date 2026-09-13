use csv;
use serde::Deserialize;
use std::error;
use calamine::{open_workbook_auto,Error, RangeDeserializerBuilder, Reader};
use std::path::Path;

#[allow(dead_code)]
pub enum FileType {
    Excel,
    Csv,
    Auto,
}

#[derive(Deserialize, Debug)]
pub struct EmployeeRecord {
    pub email: Option<String>,
    pub password: Option<String>,
}

fn get_data_excel(file_path: &Path) ->  Result<Vec<EmployeeRecord>, Box<dyn error::Error>> {
    let mut workbook = open_workbook_auto(file_path)?;


    let range = workbook
        .worksheet_range_at(0)
        .ok_or(Error::Msg("لم يتم العثور على ورقة العمل"))??;


    let iter = RangeDeserializerBuilder::new().from_range(&range)?;


    let mut records: Vec<EmployeeRecord> = Vec::new();


    for result in iter {
        let record: EmployeeRecord = result?; 
        records.push(record);
    }


    Ok(records)
    
}


fn get_data_csv(file_path: &Path) -> Result<Vec<EmployeeRecord>, Box<dyn error::Error>>{

    let mut reader = csv::Reader::from_path(file_path)?;


    let mut records: Vec<EmployeeRecord> = Vec::new();


    for result in reader.deserialize() {
        let record: EmployeeRecord = result?; 
        records.push(record);
    }

    Ok(records)
}


pub fn get_data(file_path: &Path, file_type: FileType) -> Result<Vec<EmployeeRecord>, Box<dyn error::Error>>{
    match file_type {
        FileType::Auto => {
            let is_csv = file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .eq_ignore_ascii_case("csv");

            if is_csv {
                get_data_csv(file_path)
            } else {
                get_data_excel(file_path)
            }
        },
        FileType::Excel => get_data_excel(file_path),
        FileType::Csv => get_data_csv(file_path),
    }
}
