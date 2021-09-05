//! `stringsext` searches for multi-byte encoded strings in binary data.\

//! `stringsext` is a Unicode enhancement of the GNU strings tool with
//! additional functionalities: stringsext recognizes Cyrillic, CJKV characters
//! and other scripts in all supported multi-byte-encodings, while GNU strings
//! fails in finding any of these scripts in UTF-16 and many other encodings.\

//! The role of the main-module is to launch the processing of the input stream in
//! batches with threads. It also receives, merges, sorts and prints the results.

//!  # Operating principle

//!  1. The iterator `input::Slicer` concatenates the input-files and cuts
//!  the input stream into slices called `main::slice`.
//!
//!  2. In `main::run()` these slices are feed in parallel to threads, where each has
//!  its own `Mission` configuration.
//!
//!  3. Each thread runs a search in `main::slice` == `scanner::input_buffer`. The
//!  search is performed by `scanner::FindingCollection::scan()`, which cuts the `scanner::input_buffer`
//!  into smaller chunks of size 2*`output_line_char_nb_max` bytes hereafter called
//! `input_window`.
//!
//!  4. The `Decoder` runs through the `input_window`, searches for valid strings and
//!  decodes them into UTF-8-chunks.
//!
//!  5. Each UTF-8-chunk is then fed into the filter `helper::SplitStr` to be
//!  analyzed if parts of it satisfy certain filter conditions.
//!
//!  6. Doing so, the `helper::SplitStr` cuts the UTF-8-chunk into even smaller
//!  `SplitStr`-chunks not longer than `output_line_char_nb_max` and sends them back to the
//!  `scanner::FindingCollection::scan()` loop.
//!
//!  7. There the `SplitStr`-chunk is packed into a `finding::Finding` object and
//!  then successively added to a `finding::FindingCollection`.
//!
//!  8. After finishing its run through the `input_window` the search continues with
//!  the next `input_window. Goto 5.
//!
//!  9. When all `input_window` s are processed, `scanner::FindingCollection::scan()` returns the
//!  `finding::FindingCollection` to `main::run()` and exits.
//!
//!  10. `main::run()` waits for all threads to return their
//!  `finding::FindingCollection` s. Then, all `Findings` s are merged,
//!  sorted and finally print out by `finding::print()`.
//!
//!  11. While the print still running, the next `main::slice` ==
//!  `scanner::input_buffer` is sent to all threads for the next search.
//!  Goto 3.
//!
//!  12. `main::run()` exits when all `main::slice` s are processed.

extern crate encoding_rs;

mod finding;
mod finding_collection;
mod help;
mod helper;
mod input;
mod mission;
mod options;
mod scanner;

use crate::finding::OUTPUT_LINE_METADATA_LEN;
use crate::finding_collection::FindingCollection;
use crate::help::help;
use crate::input::Slicer;
use crate::mission::MISSIONS;
use crate::options::ARGS;
use crate::scanner::ScannerStates;
use itertools::kmerge;
use scoped_threadpool::Pool;
use std::fs::File;
use std::io;
use std::io::LineWriter;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::process;
use std::str;
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;

/// Processes the input stream in batches with threads. Then receives, merges, sorts and
/// prints the result

fn run() -> Result<(), anyhow::Error> {
    let merger: JoinHandle<_>;
    // Scope for threads
    {
        let n_threads = MISSIONS.len();
        let (tx, rx) = mpsc::sync_channel(n_threads);
        //
        // Receiver thread:

        // Receive `FindingCollection`s from scanner threads.
        merger = thread::spawn(move || {
            // Set up output channel.
            let mut output = match ARGS.output {
                Some(ref fname) => {
                    let f = File::create(&Path::new(fname))?;
                    // There is at least one `Mission` in `MISSIONS`.
                    let output_line_len =
                        2 * MISSIONS[0].output_line_char_nb_max + OUTPUT_LINE_METADATA_LEN;
                    let f = LineWriter::with_capacity(output_line_len, f);
                    Box::new(f) as Box<dyn Write>
                }
                None => Box::new(io::stdout()) as Box<dyn Write>,
            };
            output.write_all("\u{feff}".as_bytes())?;

            'batch_receiver: loop {
                // collect
                let mut results: Vec<Pin<Box<FindingCollection>>> = Vec::with_capacity(n_threads);
                for _ in 0..n_threads {
                    results.push(match rx.recv() {
                        // It would be safe to unpin here, as only read operations on data follow:
                        //       Ok(fc) => unsafe { *Pin::into_inner_unchecked(fc) },
                        // Instead, we implement `IntoIterator` for the pinned `Pin<Box<FindingCollection>>` type,
                        // allowing us to `kmerge` a vector of type `Vec<Pin<Box<FindingCollection>>>`.
                        // In this way no unsafe is needed.
                        Ok(fc) => fc,
                        _ => break 'batch_receiver,
                    });
                }
                // merge
                for finding in kmerge(&results) {
                    finding.print(&mut output)?;
                }
            }
            //println!("Merger terminated.");
            output.write_all(&[b'\n'])?;
            output.flush()?;
            Ok(())
        });

        //
        // Sender threads:

        // Setting up the data slice producer.
        let input = Slicer::new();

        // We set up the processor.
        let mut sss = ScannerStates::new(&MISSIONS);
        let mut pool = Pool::new(MISSIONS.len() as u32);

        for (slice, input_file_id, is_last_input_buffer) in input {
            pool.scoped(|scope| {
                for mut ss in sss.v.iter_mut() {
                    let tx = tx.clone();
                    scope.execute(move || {
                        let fc = FindingCollection::from(
                            &mut ss,
                            input_file_id,
                            slice,
                            is_last_input_buffer,
                        );
                        // Send the result to the receiver thread.
                        tx.send(fc).expect(
                            "Error: Can not sent result through output channel. \
                             Write permissions? Is there enough space? ",
                        );
                    });
                }
            });
        }
    } // `tx` drops here, which breaks the `batch_receiver`-loop.

    // If everything goes well, we get `()` here.
    merger.join().unwrap()

    // All threads terminated.
}

/// Application entry point.
fn main() {
    help();

    if let Err(e) = run() {
        eprintln!("Error: `{:?}`.", e);
        process::exit(1);
    }
}
