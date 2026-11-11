//! Native playback with rodio. The output device is opened once, on a thread of its own (cpal
//! streams must stay on the thread that made them), and kept open so that sounds start at once.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::decoder::DecoderBuilder;
use rodio::{Decoder, MixerDeviceSink, Player as RodioPlayer};

use crate::error::{AppError, AppResult};
use crate::models::PlaybackState;

enum Command {
    Play { request: PlayRequest, reply: Sender<AppResult<()>> },
}

pub struct PlayRequest {
    pub sound_id: i64,
    pub path: PathBuf,
    pub start: f64,
    pub looping: bool,
    pub volume: f32,
    pub duration: f64,
}

struct Current {
    sound_id: i64,
    player: RodioPlayer,
    looping: bool,
    duration: f64,
}

pub struct Player {
    tx: Mutex<Sender<Command>>,
    current: Arc<Mutex<Option<Current>>>,
}

fn audio_error(e: impl std::fmt::Display) -> AppError {
    AppError::Audio { message: e.to_string() }
}

impl Player {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let current = Arc::new(Mutex::new(None));
        let shared = current.clone();
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || audio_thread(rx, shared))
            .expect("could not start the audio thread");
        Player { tx: Mutex::new(tx), current }
    }

    pub fn play(&self, request: PlayRequest) -> AppResult<()> {
        let (reply, answer) = mpsc::channel();
        self.tx
            .lock()
            .unwrap()
            .send(Command::Play { request, reply })
            .map_err(|_| audio_error("The audio thread has stopped."))?;
        answer
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| audio_error("The audio device does not respond."))?
    }

    pub fn stop(&self) {
        if let Some(c) = self.current.lock().unwrap().take() {
            c.player.stop();
        }
    }

    pub fn toggle_pause(&self) -> bool {
        let current = self.current.lock().unwrap();
        match current.as_ref() {
            Some(c) if c.player.is_paused() => {
                c.player.play();
                true
            }
            Some(c) => {
                c.player.pause();
                false
            }
            None => false,
        }
    }

    pub fn seek(&self, seconds: f64) -> AppResult<()> {
        let current = self.current.lock().unwrap();
        if let Some(c) = current.as_ref() {
            c.player.try_seek(Duration::from_secs_f64(seconds.max(0.0))).map_err(audio_error)?;
        }
        Ok(())
    }

    pub fn set_volume(&self, volume: f32) {
        if let Some(c) = self.current.lock().unwrap().as_ref() {
            c.player.set_volume(volume.clamp(0.0, 1.0));
        }
    }

    pub fn state(&self) -> PlaybackState {
        let current = self.current.lock().unwrap();
        match current.as_ref() {
            Some(c) if !c.player.empty() => {
                let mut position = c.player.get_pos().as_secs_f64();
                if c.looping && c.duration > 0.0 {
                    position %= c.duration;
                }
                PlaybackState {
                    sound_id: Some(c.sound_id),
                    playing: !c.player.is_paused(),
                    position,
                    looping: c.looping,
                }
            }
            _ => PlaybackState { sound_id: None, playing: false, position: 0.0, looping: false },
        }
    }
}

fn decoder(path: &PathBuf) -> AppResult<DecoderBuilder<BufReader<File>>> {
    let file = File::open(path).map_err(audio_error)?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut builder =
        Decoder::builder().with_data(BufReader::new(file)).with_byte_len(len).with_seekable(true);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        builder = builder.with_hint(&ext.to_ascii_lowercase());
    }
    Ok(builder)
}

fn cannot_play(e: impl std::fmt::Display) -> AppError {
    audio_error(format!("Cannot play this file: {e}"))
}

fn audio_thread(rx: Receiver<Command>, current: Arc<Mutex<Option<Current>>>) {
    let mut device: Option<MixerDeviceSink> = None;
    let open_device = || {
        rodio::DeviceSinkBuilder::open_default_sink().map(|mut d| {
            d.log_on_drop(false);
            d
        })
    };
    // Open the device right away, so that the first sound starts without delay.
    device = open_device().ok().or(device);
    for command in rx {
        match command {
            Command::Play { request, reply } => {
                if device.is_none() {
                    match open_device() {
                        Ok(d) => device = Some(d),
                        Err(e) => {
                            let _ = reply.send(Err(audio_error(format!("No audio output: {e}"))));
                            continue;
                        }
                    }
                }
                let mixer = device.as_ref().unwrap().mixer();
                let result = start(&request, mixer).map(|player| {
                    let mut current = current.lock().unwrap();
                    if let Some(old) = current.take() {
                        old.player.stop();
                    }
                    *current = Some(Current {
                        sound_id: request.sound_id,
                        player,
                        looping: request.looping,
                        duration: request.duration,
                    });
                });
                let _ = reply.send(result);
            }
        }
    }
}

fn start(request: &PlayRequest, mixer: &rodio::mixer::Mixer) -> AppResult<RodioPlayer> {
    let player = RodioPlayer::connect_new(mixer);
    player.set_volume(request.volume.clamp(0.0, 1.0));
    let builder = decoder(&request.path)?;
    if request.looping {
        player.append(builder.build_looped().map_err(cannot_play)?);
    } else {
        player.append(builder.build().map_err(cannot_play)?);
    }
    if request.start > 0.0 {
        let _ = player.try_seek(Duration::from_secs_f64(request.start));
    }
    Ok(player)
}
