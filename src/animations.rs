use crate::log_debug;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

// Embed image resources
const JUNGLE_IMAGE: &[u8] = include_bytes!("../art/jungle.jpg");

pub fn get_embedded_image(path: &str) -> Option<&'static [u8]> {
    match path {
        "art/jungle.jpg" => Some(JUNGLE_IMAGE),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Spell {
    pub trigger: String,
    pub duration_ms: u64,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Frame {
    pub frame: u64, // Time in ms when this frame appears
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub overlay: bool, // If true, this frame is an overlay on top of previous content
    #[serde(default = "default_blink_rate")]
    pub blink_rate_ms: u64, // Blink rate in milliseconds, defaults to 200ms
}

fn default_blink_rate() -> u64 {
    200
}

#[derive(Debug, Clone)]
pub struct ActiveAnimation {
    pub spell: Spell,
    pub start_time: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct AnimationEngine {
    pub spells: HashMap<String, Spell>,
    pub active_animation: Option<ActiveAnimation>,
}

impl AnimationEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_spells(&mut self) -> Result<()> {
        log_debug!("ANIMATION: Starting to load spells from animations/spells.yaml");

        // Try to load from filesystem first, fallback to embedded
        let content = match std::fs::read_to_string("animations/spells.yaml") {
            Ok(content) => {
                log_debug!("ANIMATION: Loaded spells from filesystem");
                content
            }
            Err(_) => {
                log_debug!("ANIMATION: Loading spells from embedded resources");
                include_str!("../animations/spells.yaml").to_string()
            }
        };

        log_debug!("ANIMATION: Read {} bytes from spells.yaml", content.len());

        let spells: HashMap<String, Spell> =
            serde_yaml::from_str(&content).context("Failed to parse spells.yaml")?;

        log_debug!(
            "ANIMATION: Loaded {} spells: {:?}",
            spells.len(),
            spells.keys().collect::<Vec<_>>()
        );

        for (key, spell) in &spells {
            log_debug!(
                "ANIMATION: Spell '{}' has trigger '{}' with {} frames",
                key,
                spell.trigger,
                spell.frames.len()
            );
        }

        self.spells = spells;
        log_debug!("ANIMATION: Spell loading complete");
        Ok(())
    }

    pub fn trigger(&mut self, trigger_name: &str) {
        log_debug!("ANIMATION: Triggering animation: '{}'", trigger_name);
        log_debug!(
            "ANIMATION: Available spells: {:?}",
            self.spells.keys().collect::<Vec<_>>()
        );

        if let Some(spell) = self.spells.get(trigger_name).cloned() {
            log_debug!(
                "ANIMATION: Found spell '{}' with trigger '{}' and {} frames",
                trigger_name,
                spell.trigger,
                spell.frames.len()
            );
            self.active_animation = Some(ActiveAnimation {
                spell,
                start_time: Instant::now(),
            });
            log_debug!("ANIMATION: Animation started successfully");
        } else {
            log_debug!("ANIMATION: ERROR - Spell not found: '{}'", trigger_name);
        }
    }

    pub fn get_current_frame(&self) -> Option<String> {
        if let Some(ref active) = self.active_animation {
            let elapsed = active.start_time.elapsed().as_millis() as u64;

            log_debug!(
                "ANIMATION: get_current_frame - elapsed: {}ms, duration: {}ms",
                elapsed,
                active.spell.duration_ms
            );

            // Animation finished?
            if elapsed > active.spell.duration_ms {
                log_debug!("ANIMATION: Animation expired");
                return None;
            }

            // Find the current frame (skip overlay frames)
            let mut current_frame = None;
            for frame in &active.spell.frames {
                if !frame.overlay && elapsed >= frame.frame {
                    current_frame = Some(frame);
                    log_debug!("ANIMATION: Using frame at {}ms", frame.frame);
                }
            }

            if let Some(frame) = current_frame {
                // Load image using viu if specified
                if let Some(ref image_path) = frame.image {
                    log_debug!("ANIMATION: Loading image from file: {}", image_path);
                    // Check if we have embedded version or if file exists
                    let has_embedded = get_embedded_image(image_path).is_some();
                    let file_exists = std::path::Path::new(image_path).exists();

                    if has_embedded || file_exists {
                        if let Ok(ansi_output) = render_image_to_ansi(image_path) {
                            log_debug!("ANIMATION: Image marker created: {}", &ansi_output);
                            return Some(ansi_output);
                        } else {
                            log_debug!("ANIMATION: ERROR - Failed to render image: {}", image_path);
                        }
                    } else {
                        log_debug!(
                            "ANIMATION: ERROR - Image not found (embedded or filesystem): {}",
                            image_path
                        );
                    }
                }

                // Load from file if specified
                if let Some(ref file_path) = frame.file {
                    log_debug!("ANIMATION: Loading frame from file: {}", file_path);
                    if let Ok(content) = std::fs::read_to_string(file_path) {
                        return Some(content);
                    } else {
                        log_debug!("ANIMATION: ERROR - Failed to read file: {}", file_path);
                    }
                }

                // Otherwise use text
                if let Some(ref text) = frame.text {
                    log_debug!("ANIMATION: Using text frame ({} chars)", text.len());
                    return Some(text.clone());
                } else {
                    log_debug!("ANIMATION: ERROR - Frame has no text, file, or image");
                }
            } else {
                log_debug!("ANIMATION: No frame found for elapsed time {}ms", elapsed);
            }
        }
        None
    }

    pub fn get_overlay_frame(&self) -> Option<String> {
        if let Some(ref active) = self.active_animation {
            let elapsed = active.start_time.elapsed().as_millis() as u64;
            log_debug!(
                "ANIMATION: get_overlay_frame called - elapsed: {}ms",
                elapsed
            );

            // Find current overlay frame if any
            for frame in &active.spell.frames {
                log_debug!(
                    "ANIMATION: Checking frame - overlay: {}, elapsed: {}ms, frame_time: {}ms",
                    frame.overlay,
                    elapsed,
                    frame.frame
                );

                if frame.overlay && elapsed >= frame.frame {
                    if let Some(ref text) = frame.text {
                        // Simple reliable blinking: get current instant and use it for blink timing
                        let now = std::time::Instant::now();
                        let millis_since_epoch =
                            now.duration_since(active.start_time).as_millis() as u64;

                        let blink_cycle = millis_since_epoch / frame.blink_rate_ms;
                        let show_text = blink_cycle % 2 == 0;

                        log_debug!("ANIMATION: BLINK DEBUG - since_start: {}ms, cycle: {}, rate: {}ms, show: {}", 
                                  millis_since_epoch, blink_cycle, frame.blink_rate_ms, show_text);

                        if show_text {
                            log_debug!("ANIMATION: SHOWING TEXT");
                            return Some(text.clone());
                        } else {
                            log_debug!("ANIMATION: HIDING TEXT");
                            return None;
                        }
                    } else {
                        log_debug!("ANIMATION: Overlay frame has no text");
                    }
                } else {
                    log_debug!(
                        "ANIMATION: Frame not ready - overlay: {}, elapsed >= frame: {}",
                        frame.overlay,
                        elapsed >= frame.frame
                    );
                }
            }
            log_debug!("ANIMATION: No overlay frames found");
        } else {
            log_debug!("ANIMATION: No active animation");
        }
        None
    }

    pub fn is_active(&self) -> bool {
        if let Some(ref active) = self.active_animation {
            let elapsed = active.start_time.elapsed().as_millis() as u64;
            elapsed <= active.spell.duration_ms
        } else {
            false
        }
    }

    pub fn update(&mut self) {
        // Clear expired animations
        if let Some(ref active) = self.active_animation {
            let elapsed = active.start_time.elapsed().as_millis() as u64;
            if elapsed > active.spell.duration_ms {
                log_debug!("ANIMATION: Animation expired, clearing");
                self.active_animation = None;
            }
        }
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.active_animation = None;
    }
}

fn render_image_to_ansi(image_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Return a marker that indicates this is an image to be rendered with ratatui-image
    Ok(format!("IMAGE:{}", image_path))
}
