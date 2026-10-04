use std::{
    io::{self, BufReader, BufWriter, Write},
    sync::Arc,
    thread,
};

use crate::{
    Message as _, SourceLang, WriteResult,
    msg::{AddonMessage, Handshake, ModuleMessage},
    util::ResultEx as _,
};

pub fn start(
    source_lang: SourceLang,
    result_version: u32,
    cache_key: Arc<str>,
) -> WriteResult<(flume::Sender<ModuleMessage>, flume::Receiver<AddonMessage>)> {
    // Handshake.
    {
        let mut out = io::stdout().lock();
        Handshake::new(source_lang as i32, result_version, cache_key)?.write(&mut out)?;
        out.flush()?;
    }

    // Reader thread: stdin.
    let (in_tx, in_rx) = flume::unbounded::<AddonMessage>();
    thread::spawn(move || {
        let mut input = BufReader::new(io::stdin().lock());
        while let Ok(msg) = AddonMessage::read(&mut input).log_error() {
            if in_tx.send(msg).log_error().is_err() {
                break;
            }
        }
        log::info!("stdin closed or bad frame, exiting");
        std::process::exit(0); // stdin closed.
    });

    // Writer thread: replies -> stdout, flushed once per burst.
    let (out_tx, out_rx) = flume::unbounded::<ModuleMessage>();
    thread::spawn(move || {
        let mut out = BufWriter::new(io::stdout().lock());
        while let Ok(msg) = out_rx.recv() {
            let mut ok = msg.write(&mut out).log_error().is_ok();
            while let Ok(msg) = out_rx.try_recv() {
                ok &= msg.write(&mut out).log_error().is_ok();
            }
            if !ok || out.flush().log_error().is_err() {
                break; // Pipe closed.
            }
        }
        log::info!("stdout closed, exiting");
        std::process::exit(0);
    });

    Ok((out_tx, in_rx))
}
