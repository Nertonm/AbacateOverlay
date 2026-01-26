
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::io::Cursor;
use xcap::Monitor;
use tracing::{info, debug, error, instrument};
use std::env;
use image::{DynamicImage, ImageFormat, RgbaImage, ImageBuffer, Rgba};

fn max_frame_size() -> usize {
    std::env::var("MAX_FRAME_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20 * 1024 * 1024)
}

/// Adapter trait: Converts captured frames to RGBA byte format
pub trait FrameToRgba {
    /// returns (width, height, rgba_bytes) where rgba_bytes.len() == (w*h*4)
    fn to_rgba_bytes(&self) -> Result<(u32, u32, Vec<u8>), String>;
}

// Implement the adapter trait for ImageBuffer<Rgba<u8>, Vec<u8>> returned by xcap
impl FrameToRgba for ImageBuffer<Rgba<u8>, Vec<u8>> {
    fn to_rgba_bytes(&self) -> Result<(u32, u32, Vec<u8>), String> {
        let w = self.width();
        let h = self.height();
        // as_raw returns &Vec<u8> containing pixel data in row-major RGBA order
        Ok((w, h, self.as_raw().to_vec()))
    }
}

// Frontend can call this command to start streaming screen data to the specified host and port
#[tauri::command]
async fn capture_and_stream() -> Result<String, String> {
    info!("capture_and_stream started");

    // Identify the monitors to capture
    let monitors = Monitor::all().map_err(|e| e.to_string())?;

    // Capture the principal monitor (index 0)
    let monitor = monitors.first().ok_or("No monitors found")?;
    // Capture the screen (xcap uses capture_image)
    let frame = monitor.capture_image().map_err(|e| e.to_string())?;

    let (w, h, rgba_bytes): (u32, u32, Vec<u8>) = frame.to_rgba_bytes()?;
    info!("Captured frame: {}x{}, {} bytes", w, h, rgba_bytes.len());

    let expected = (w as usize) 
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(4))
        .ok_or("Frame size overflow")?;
    if rgba_bytes.len() != expected {
        return Err(format!("Unexpected RGBA byte length: got {}, expected {}", rgba_bytes.len(), expected));
    }
    if rgba_bytes.len() > max_frame_size() {
        return Err(format!("Frame size {} exceeds maximum allowed {}", rgba_bytes.len(), max_frame_size()));
    }

    // Create an image from the RGBA bytes
    let img = RgbaImage::from_raw(w, h, rgba_bytes)
        .ok_or("Failed to create image from RGBA bytes")?;
    let dyn_img = DynamicImage::ImageRgba8(img);

    // Encode the DynamicImage once into PNG bytes
    let mut bytes: Vec<u8> = Vec::new();
    dyn_img
    .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    let total_size = bytes.len() as u32;
    info!("Captured frame size: {} bytes", total_size);


    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(42069u16);

    // Connect to the specified host and port
    let addr = format!("{}:{}", host, port);
    info!(%addr, "Connecting to sidecar");
    let mut stream = TcpStream::connect(&addr).await
        .map_err(|e| e.to_string())?;

    // Send Header (Big-endian u32 size)
    stream.write_all(&total_size.to_be_bytes()).await
        .map_err(|e| e.to_string())?;
    info!("Sent header with size: {} bytes", total_size);

    // Send the image data
    stream.write_all(&bytes).await
        .map_err(|e| e.to_string())?;

    info!("Sent frame of {} bytes successfully", total_size);

    Ok(format!("Streamed frame of {} bytes to {}:{}", total_size, host, port))

}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![capture_and_stream]) // Registra o comando
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}