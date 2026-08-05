
//
// SC
//
// Drives a `sclang` subprocess as a DeltaConsumer, controlling the patch in
// sc/main.scd over its stdin instead of over the network.
//
// sclang only evaluates piped stdin at all when it's launched *without* a
// script-file argument (passing one runs the file correctly once, then
// stops reading stdin as code entirely) -- and even then, its interactive
// reader evaluates one newline-terminated line at a time rather than a
// whole balanced multi-line block, so a multi-line patch sent as-is gets
// shredded into broken partial statements. The workaround: launch bare
// `sclang`, and flatten the patch to a single line before sending it as the
// first command -- everything after that (~noteOn.(60); etc.) is already a
// self-contained one-liner and needs no special handling.
//

use std::io::{self,BufRead,BufReader,Write};
use std::process::{Child,ChildStdin,ChildStdout,Command,Stdio};
use std::sync::mpsc::{self,Receiver};
use std::thread;
use std::time::Duration;

use crate::output::DeltaConsumer;
use crate::zgicabra::{DeltaEvent,SignalState};

const PATCH_SOURCE: &str = include_str!("../sc/main.scd");
const READY_SENTINEL: &str = "ZGICABRA_READY";
const BOOT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct ScOutput {
    child: Child,
    stdin: ChildStdin,
}

impl ScOutput {
    // `log_output` controls whether sclang's stdout (its boot log, and an
    // echo of every command sent to it) gets printed. It's driven by
    // whether the TUI is active: those lines would otherwise scroll past
    // and corrupt the TUI's fixed-position cursor drawing, since it's
    // printed continuously (once per command, i.e. every frame) rather
    // than just at boot.
    pub fn new (log_output: bool) -> io::Result<ScOutput> {
        println!("║ Booting SuperCollider (sclang)... ");

        let mut child = Command::new("sclang")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("sclang not found on PATH ({e}); is SuperCollider installed?")))?;

        let stdin  = child.stdin.take().expect("child stdin was piped");
        let stdout = child.stdout.take().expect("child stdout was piped");

        let ready_rx = spawn_stdout_relay(stdout, log_output);

        let mut output = ScOutput { child, stdin };
        output.eval(PATCH_SOURCE);

        if ready_rx.recv_timeout(BOOT_TIMEOUT).is_err() {
            let _ = output.child.kill();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "SuperCollider didn't report ready in time"));
        }

        println!("║ SuperCollider connection OK.");

        Ok(output)
    }

    // sclang's stdin reader evaluates one line at a time, so any embedded
    // newlines are flattened to spaces -- this is what lets the whole
    // (deliberately comment-light) patch go over in one call.
    fn eval (&mut self, code: &str) {
        let flat = code.replace('\n', " ");
        let _ = self.stdin.write_all(flat.as_bytes());
        let _ = self.stdin.write_all(b"\n");
        let _ = self.stdin.flush();
    }
}

// Drains the child's stdout for as long as the process lives, printing every
// line so SC's own log stays visible. This has to keep running for the
// entire session, not just until boot completes: sclang echoes every command
// it evaluates back to stdout, and an OS pipe's buffer is finite (~64KB on
// macOS) -- if nothing keeps reading it, it fills up, sclang's next stdout
// write blocks, and since sclang is single-threaded that stalls its stdin
// reader too, silently dropping every subsequent command (this is exactly
// what caused notes to go out but never sound: the boot log alone nearly
// fills the pipe, so it doesn't take much more before sclang wedges).
//
// The one-shot ready signal rides the same relay rather than a second
// mechanism, via an Option<Sender> that's fired once then dropped so the
// print loop below isn't disturbed by it going away. `log_output` gates only
// the printing -- the thread keeps draining stdout either way, since that's
// what keeps the pipe from filling up (see the comment above).
fn spawn_stdout_relay (stdout: ChildStdout, log_output: bool) -> Receiver<()> {
    let (ready_tx, ready_rx) = mpsc::channel();

    thread::spawn(move || {
        let mut ready_tx = Some(ready_tx);

        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };

            if log_output {
                println!("║ [sc] {line}");
            }

            if line.trim() == READY_SENTINEL {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(());
                }
            }
        }
    });

    ready_rx
}

impl DeltaConsumer for ScOutput {
    fn panic (&mut self) {
        self.eval("~noteOff.();");
    }

    fn handle_signal (&mut self, signal: &SignalState) {
        self.eval(&format!("~setFilter.({});", signal.filter));
    }

    fn handle_event (&mut self, delta: &DeltaEvent) {
        match delta {
            DeltaEvent::NoteStart(note)         => self.eval(&format!("~noteOn.({note});")),
            DeltaEvent::NoteChange(_, new_note)  => self.eval(&format!("~noteOn.({new_note});")),
            DeltaEvent::NoteEnd(_)               => self.eval("~noteOff.();"),
            DeltaEvent::Panic()                  => self.eval("~noteOff.();"),
            _ => {},
        }
    }
}

impl Drop for ScOutput {
    fn drop (&mut self) {
        self.eval("~shutdown.();");
        let _ = self.child.wait();
    }
}
