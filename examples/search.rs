use stringsext::finding_collection::FindingCollection;
use stringsext::mission::Missions;
use stringsext::scanner::ScannerState;

use std::pin::Pin;

use lazy_static::lazy_static;

lazy_static! {
    pub static ref MISSIONS: Missions = Missions::new(
        Some("5000".into()).as_ref(),
        &["ascii".to_string(), "utf-8".to_string()],
        Some("5".into()).as_ref(),
        true,
        None,
        None,
        None,
        Some("30".into()).as_ref(),
    )
    .unwrap();
}

fn main() {
    let missions = &MISSIONS;

    let file_name = std::env::args().nth(1).unwrap();

    println!("{:?}", file_name);

    let inp_string = std::fs::read_to_string(&file_name).unwrap();

    let inp = inp_string.as_bytes();

    let mut ss0 = ScannerState::new(&missions.v[0]);
    let mut ss1 = ScannerState::new(&missions.v[1]);

    let mut resv: Vec<Pin<Box<FindingCollection>>> = Vec::new();
    let fc = FindingCollection::from(&mut ss0, Some(0), inp, true);
    resv.push(fc);
    let fc = FindingCollection::from(&mut ss1, Some(0), inp, true);
    resv.push(fc);

    println!("{:?}", resv);
}
