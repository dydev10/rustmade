use std::{os::raw::c_void, ptr::null_mut, sync::{Mutex, OnceLock}};

use windows::{
    core::*,
    Win32::{Foundation::*, Graphics::Gdi::*, System::{LibraryLoader::*, Memory::{VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE}}, UI::{Input::XboxController::*, WindowsAndMessaging::*}},
};

// TODO: these GamepadX structs should be defined in core game code, not platform layer
#[derive(Default)]
struct GamepadButtons {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    start: bool,
    back: bool,
    l_thumb: bool,
    r_thumb: bool,
    l_shoulder: bool,
    r_shoulder: bool,
    a: bool,
    b: bool,
    x: bool,
    y: bool,
}

#[derive(Default)]
struct GamepadTriggers {
    l_trigger: u8,
    r_trigger: u8,
}

#[derive(Default)]
struct GamepadSticks {
    l_stick_x: i16,
    l_stick_y: i16,
    r_stick_x: i16,
    r_stick_y: i16,
}

#[derive(Default)]
struct GamepadState {
    buttons: GamepadButtons,
    triggers: GamepadTriggers,
    sticks: GamepadSticks,
}

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

static mut GLOBAL_RUNNING: bool = false;
static mut GLOBAL_BUFFER: *mut Win32OffscreenBuffer = null_mut();
static GLOBAL_GAMEPAD_0: OnceLock<Mutex<GamepadState>> = OnceLock::new();

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
        WM_CLOSE => {
            unsafe {
                GLOBAL_RUNNING = false;
            }
            LRESULT(0)
        }
        WM_QUIT => {
            unsafe {
                GLOBAL_RUNNING = false;
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

fn render_gradient(buffer: &mut Win32OffscreenBuffer, x_offset: i32, y_offset: i32) {
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
                let b = (x + x_offset) as u8;
                let g = (y + y_offset) as u8;
                let r = 0u8;
                let a = 255u8;
                let pixel = (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32);
                *pixel_ptr.add(offset) = pixel;
            }
        }
    }
}

/*
    called every frame to read the latest controller state

    TODO: use dynamic linking of xinput1_x.dll, 1_4 is linked by windows crate but only 1_3 or other version may be available on older windows
*/
fn read_controller_state() {
    // TODO: either track more controller states or remove this loop to just read first controller
    // second controller will overwrite the inputs if connected
    for controller_index in 0..XUSER_MAX_COUNT {
        let mut controller_state: XINPUT_STATE = XINPUT_STATE::default();
        unsafe {
            // ERROR_SUCCESS means success in windows api
            if XInputGetState(controller_index, &mut controller_state) == ERROR_SUCCESS.0 {
                // TODO: check if controllerState.dwPacketNumber is not increasing too much, should be same or +1(or very very low if not 1) for each poll
                let gamepad: XINPUT_GAMEPAD = controller_state.Gamepad;
                let mut gamepad_state = GLOBAL_GAMEPAD_0.get().expect("Gamepad state not initialized")
                                                                        .lock().expect("failed to lock gamepad state before updating");

                gamepad_state.buttons.up = (gamepad.wButtons & XINPUT_GAMEPAD_DPAD_UP).0 > 0;
                gamepad_state.buttons.down = (gamepad.wButtons & XINPUT_GAMEPAD_DPAD_DOWN).0 > 0;
                gamepad_state.buttons.left = (gamepad.wButtons & XINPUT_GAMEPAD_DPAD_LEFT).0 > 0;
                gamepad_state.buttons.right = (gamepad.wButtons & XINPUT_GAMEPAD_DPAD_RIGHT).0 > 0;
                gamepad_state.buttons.start = (gamepad.wButtons & XINPUT_GAMEPAD_START).0 > 0;
                gamepad_state.buttons.back = (gamepad.wButtons & XINPUT_GAMEPAD_BACK).0 > 0;
                gamepad_state.buttons.l_thumb = (gamepad.wButtons & XINPUT_GAMEPAD_LEFT_THUMB).0 > 0;
                gamepad_state.buttons.r_thumb = (gamepad.wButtons & XINPUT_GAMEPAD_RIGHT_THUMB).0 > 0;
                gamepad_state.buttons.l_shoulder = (gamepad.wButtons & XINPUT_GAMEPAD_LEFT_SHOULDER).0 > 0;
                gamepad_state.buttons.r_shoulder = (gamepad.wButtons & XINPUT_GAMEPAD_RIGHT_SHOULDER).0 > 0;
                gamepad_state.buttons.a = (gamepad.wButtons & XINPUT_GAMEPAD_A).0 > 0;
                gamepad_state.buttons.b = (gamepad.wButtons & XINPUT_GAMEPAD_B).0 > 0;
                gamepad_state.buttons.x = (gamepad.wButtons & XINPUT_GAMEPAD_X).0 > 0;
                gamepad_state.buttons.y = (gamepad.wButtons & XINPUT_GAMEPAD_Y).0 > 0;
            

                gamepad_state.triggers.l_trigger = gamepad.bLeftTrigger;
                gamepad_state.triggers.r_trigger = gamepad.bRightTrigger;

                gamepad_state.sticks.l_stick_x = gamepad.sThumbLX;
                gamepad_state.sticks.l_stick_y = gamepad.sThumbLY;
                gamepad_state.sticks.r_stick_x = gamepad.sThumbRX;
                gamepad_state.sticks.r_stick_y = gamepad.sThumbRY;
 
                // debug print to test buttons
                if gamepad_state.buttons.a {
                    print!("Gamepad button A pressed\n")
                }

                if gamepad_state.triggers.l_trigger > 100 {
                    println!("{:?}", gamepad_state.triggers.l_trigger);
                }

                // TODO: Check why using abs() causes panic for negative x
                if gamepad_state.sticks.l_stick_x.abs() > i16::MAX/4 {
                    println!("{:?}", gamepad_state.sticks.l_stick_x);
                }
            }
        }
    }
}

fn main() -> Result<()> {
    // TODO: add support for more controllers, only first controller supported for now
    GLOBAL_GAMEPAD_0.get_or_init(|| Mutex::new(GamepadState::default()));

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
            GLOBAL_RUNNING = true;

            let mut x_anim = 0;
            let mut y_anim = 0;
            let dc = GetDC(Some(window));

            let mut msg = MSG::default();
            while GLOBAL_RUNNING {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }

                // poll controller state
                read_controller_state();

                // render every frame at the end
                render_gradient(&mut *GLOBAL_BUFFER, x_anim, y_anim);
                let dimension = win32_get_window_dimension(window).expect("Failed GetRect from windows");
                win32_display_buffer_in_window(dc, &*GLOBAL_BUFFER, dimension.width, dimension.height);

                // test animation to make sure render buffer update and main loop is working
                x_anim += 1;
                y_anim += 2;

                // test global gamepad state
                if let Some(mutex) = GLOBAL_GAMEPAD_0.get() {
                    let gamepad_state = mutex.lock().expect("cannot lock gamepad state for reading");
                    if gamepad_state.buttons.y {
                        // increase y scrolling animation speed
                        y_anim += 10;
                    }
                }
            }
        }
    }

    Ok(())
}
