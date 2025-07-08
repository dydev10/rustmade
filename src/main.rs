use std::{ os::raw::c_void, ptr::null_mut};

use windows::{
    core::*,
    Win32::{Foundation::*, Graphics::Gdi::*, System::{LibraryLoader::*, Memory::{VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE}}, UI::WindowsAndMessaging::*},
};

struct Win32WindowDimension {
    width: i32,
    height: i32,
}

#[derive(Default)]
struct Win32OffscreenBuffer {
    bitmap_info: BITMAPINFO,
    width: i32,
    height: i32,
    pitch: i32,
    memory: *mut c_void
}

static mut GLOBAL_BUFFER: *mut Win32OffscreenBuffer = null_mut();

fn win32_get_window_dimension(window: HWND) ->  Result<Win32WindowDimension> {
    unsafe {
        let mut client_rect  = RECT {
            ..Default::default()
        };
        let _ = GetClientRect(window, &mut client_rect)?;
        let dimension = Win32WindowDimension {
            width: client_rect.right - client_rect.left,
            height: client_rect.bottom - client_rect.top
        };
        Ok(dimension)
    }
}

fn win32_resize_dib_section(buffer: &mut Win32OffscreenBuffer, width: i32, height: i32) -> Win32OffscreenBuffer {
    let bytes_per_pixel = 4;
    let buffer_size = (width * height * bytes_per_pixel) as usize;
    let pitch = width * bytes_per_pixel;
    
    let allocated_memory: *mut c_void;
    unsafe {
        if !buffer.memory.is_null() {
            let _ = VirtualFree(buffer.memory, 0, MEM_RELEASE);
        }

        let framebuffer = VirtualAlloc(
            None,
            buffer_size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );

        if framebuffer.is_null() {
            panic!("Failed to allocate framebuffer");
        } else {
            allocated_memory = framebuffer;
        }
    }

    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // negative height makes the bitmap buffer starr from top left when drawing on screen
            biPlanes: 1,
            biBitCount: 32,
            //biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };

    let back_buffer = Win32OffscreenBuffer {
        bitmap_info,
        width,
        height,
        pitch,
        memory: allocated_memory,
    };
    buffer.bitmap_info = bitmap_info;
    buffer.pitch = pitch;
    buffer.memory = allocated_memory;
    buffer.width = width;
    buffer.height = height;

    return back_buffer
}

fn win32_display_buffer_in_window(device_context: HDC, buffer: &Win32OffscreenBuffer, window_width: i32, window_height: i32) {
    unsafe {
        StretchDIBits(
            device_context,
            0,
            0,
            window_width,
            window_height,
            0,
            0,
            buffer.width,
            buffer.height,
            Some(buffer.memory),
            &buffer.bitmap_info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            println!("Key down: {:?}", wparam.0);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            println!("Mouse click!");
            LRESULT(0)
        }
        WM_PAINT => {
            unsafe {
                if GLOBAL_BUFFER.is_null() {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }

                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                // You can draw using hdc here

                let dimension = win32_get_window_dimension(hwnd).expect("Failed GetRect from windows");
                win32_display_buffer_in_window(hdc, &*GLOBAL_BUFFER, dimension.width, dimension.height);

                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        _ => unsafe {
           DefWindowProcW(hwnd, msg, wparam, lparam)  
        }
    }
}

fn render_gradient(buffer: &mut Win32OffscreenBuffer) {
    let pixel_ptr = buffer.memory as *mut u32;
    unsafe {
        // Fill with gradient
        for y in 0..buffer.height {
            for x in 0..buffer.width {
                let offset = (y * buffer.width + x) as usize;
                /*
                    offset          : +0 +1 +2 +3
                    Pixel in memory : 00 00 00 00
                    Channel         : BB GG RR xx (reversed little endian because windows reverses it to look like 0x xxRRGGBB)

                    in 32bit Register     : xx RR GG BB
                    this is why void pointer is cast to u32 to fill it and move to next pixel
                */
                let b = x as u8;
                let g = y as u8;
                let r = 0u8;
                let a = 255u8;
                let pixel = (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32);
                *pixel_ptr.add(offset) = pixel;
            }
        }
    }
}

fn main() -> Result<()> {
    unsafe {
        let default_width = 1280;
        let default_height = 720;

        let buffer_state = Box::new(Win32OffscreenBuffer::default());
        GLOBAL_BUFFER = Box::into_raw(buffer_state);
        win32_resize_dib_section(&mut *GLOBAL_BUFFER, default_width, default_height);

        let h_instance = GetModuleHandleW(None)?;
        let class_name = w!("RustmadeWindowClass");

        let wc = WNDCLASSW {
            hInstance: h_instance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wnd_proc),
            style: CS_VREDRAW|CS_HREDRAW,
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Rustmade Window"),
            WS_OVERLAPPEDWINDOW|WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(h_instance.into()),
            None,
        );

        if let Ok(window) = hwnd {
            let dc = GetDC(Some(window));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);


                render_gradient(&mut *GLOBAL_BUFFER);
                let dimension = win32_get_window_dimension(window).expect("Failed GetRect from windows");
                win32_display_buffer_in_window(dc, &*GLOBAL_BUFFER, dimension.width, dimension.height);
            }
        }
    }

    Ok(())
}
