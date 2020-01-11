#![deny(bare_trait_objects)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(non_snake_case)]
#![allow(dead_code)]
extern crate rmp_serde as rmps;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate byteorder;
extern crate num;
extern crate serious_organizer_lib;
extern crate time;
#[cfg(windows)]
extern crate winapi;
#[macro_use]
extern crate log;
#[macro_use]
extern crate windows_service;

use time::PreciseTime;

use std::ptr::{null, null_mut};
//use std::time::{Duration, Instant};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, RecvError, TryRecvError};

use std::ffi::OsString;
use std::time::{Duration, Instant};

use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Error, Read};

use winapi::shared::minwindef::{DWORD, FALSE, LPCVOID, LPVOID, TRUE};
use winapi::shared::ntdef::HANDLE;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::fileapi::{ReadFile, WriteFile};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::minwinbase::SECURITY_ATTRIBUTES;
use winapi::um::namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe};
use winapi::um::winbase::{PIPE_ACCESS_DUPLEX, PIPE_READMODE_BYTE, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE};
use windows_service::service::*;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use serious_organizer_lib::lens::{SortColumn, SortOrder};
use serious_organizer_lib::{dir_search, lens, store};

use rmps::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};

use simplelog::*;

pub mod data;
pub mod wstring;

use crate::data::*;
use crate::wstring::{to_string, to_wnocstring, to_wstring};

const SERVICE_NAME: &str = "SeriousService";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

const BUFFER_SIZE: u32 = 500 * 1024;

define_windows_service!(ffi_service_main, my_service_main);

#[cfg(not(feature = "service"))]
fn main() -> Result<(), windows_service::Error> {
    // Register generated `ffi_service_main` with the system and start the service, blocking
    // this thread until the service is stopped.
    CombinedLogger::init(vec![
        SimpleLogger::new(LevelFilter::Info, Config::default()),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            std::fs::File::create("C:\\home\\src\\serious_server.log").expect("Failed to init logger"),
        ),
    ])
    .unwrap();

    info!("Start normal");
    let (_, shutdown_rx) = mpsc::channel();

    run_requests(shutdown_rx);
    Ok(())
}

#[cfg(feature = "service")]
fn main() -> windows_service::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // println!("args {:?}", args);
    match (*args.get(1).unwrap_or(&String::from("No arguments"))).as_str() {
        "install" => {
            let service = create_or_update_service(
                ServiceAccess::QUERY_CONFIG | ServiceAccess::CHANGE_CONFIG | ServiceAccess::START,
            )?;

            let actions = vec![
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(5),
                },
                ServiceAction {
                    action_type: ServiceActionType::Restart,
                    delay: Duration::from_secs(10),
                },
                ServiceAction {
                    action_type: ServiceActionType::None,
                    delay: Duration::default(),
                },
            ];

            println!("Update failure actions");
            let failure_actions = ServiceFailureActions {
                reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(600)),
                reboot_msg: None,
                command: None,
                actions: Some(actions),
            };
            service.update_failure_actions(failure_actions)?;

            println!("Query failure actions");
            let updated_failure_actions = service.get_failure_actions()?;
            println!("{:#?}", updated_failure_actions);

            println!("Enable failure actions on non-crash failures");
            service.set_failure_actions_on_non_crash_failures(true)?;

            println!("Query failure actions on non-crash failures enabled");
            let failure_actions_flag = service.get_failure_actions_on_non_crash_failures()?;
            println!(
                "Failure actions on non-crash failures enabled: {}",
                failure_actions_flag
            );
        }

        "delete" => delete_service()?,
        "query" => show_service_config()?,
        "start" => start_service()?,
        "stop" => stop_service()?,

        _ => run_service()?,
    }

    Ok(())
}

fn create_or_update_service(service_access: ServiceAccess) -> windows_service::Result<Service> {
    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

    let service_binary_path = ::std::env::current_exe().unwrap();

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from("Serious data service"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::OnDemand,
        error_control: ServiceErrorControl::Normal,
        executable_path: service_binary_path,
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None, // run as System
        account_password: None,
    };

    println!("Create or open the service {}", SERVICE_NAME);
    let service = service_manager
        .create_service(service_info, service_access)
        .or(service_manager.open_service(SERVICE_NAME, service_access))?;

    Ok(service)
}

fn delete_service() -> windows_service::Result<()> {
    use std::thread;
    let service = create_or_update_service(ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE)?;
    let service_status = service.query_status()?;

    // Make sure service is stopped before deleting
    if service_status.current_state != ServiceState::Stopped {
        service.stop()?;
        thread::sleep(Duration::from_secs(1));
    }

    println!("Delete the service {}", SERVICE_NAME);
    service.delete()?;
    Ok(())
}

fn show_service_config() -> windows_service::Result<()> {
    let service = create_or_update_service(ServiceAccess::QUERY_CONFIG)?;
    let config = service.query_config()?;
    println!("Config:");
    println!("{:#?}", config);
    Ok(())
}

fn start_service() -> windows_service::Result<()> {
    use std::thread;

    println!("Starting service {}", SERVICE_NAME);

    let service = create_or_update_service(ServiceAccess::START | ServiceAccess::QUERY_STATUS)?;
    let service_status = service.query_status()?;

    service.start(&[""])?;
    let now = Instant::now();
    // Make sure service is stopped before deleting
    if service_status.current_state != ServiceState::Running {
        let elapsed = now.elapsed().as_secs();
        println!("Elapsed {} s", elapsed);
        if elapsed > 20 {
            panic!("Failed to start service!")
        }
        thread::sleep(Duration::from_secs(1));
    }

    println!("Started service {}", SERVICE_NAME);
    Ok(())
}

fn stop_service() -> windows_service::Result<()> {
    use std::thread;

    println!("Stopping service {}", SERVICE_NAME);

    let service = create_or_update_service(ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE)?;
    let service_status = service.query_status()?;

    // Make sure service is stopped before deleting
    if service_status.current_state != ServiceState::Stopped {
        service.stop()?;
        thread::sleep(Duration::from_secs(1));
    }

    println!("Stopped service {}", SERVICE_NAME);
    Ok(())
}

fn run_service() -> Result<(), windows_service::Error> {
    use std::{
        ffi::OsString,
        net::{IpAddr, SocketAddr, UdpSocket},
        sync::mpsc,
        time::Duration,
    };
    use windows_service::{
        define_windows_service,
        service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus},
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    // Register generated `ffi_service_main` with the system and start the service, blocking
    // this thread until the service is stopped.
    WriteLogger::init(
        LevelFilter::Trace,
        Config::default(),
        std::fs::File::create("C:\\home\\src\\SeriousOrganizerServer\\serious_server.log")
            .expect("Failed to init logger"),
    )
    .unwrap();
    // CombinedLogger::init(
    //     vec![

    //     ]
    // ).unwrap();
    info!("Start service");
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;

    Ok(())
}

fn my_service_main(arguments: Vec<OsString>) {
    use std::thread;
    use windows_service::{
        define_windows_service,
        service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher, Result,
    };

    // Create a channel to be able to poll a stop event from the service worker loop.
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            // Notifies a service to report its current status information to the service
            // control manager. Always return NoError even if not implemented.
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,

            // Handle stop
            ServiceControl::Stop => {
                shutdown_tx.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            }

            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    // Register system service event handler.
    // The returned status handle should be used to report service status changes to the system.
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler).unwrap();
    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
        })
        .expect("Failed to update ");

    run_requests(shutdown_rx);
    info!("run_requests done");

    // Tell the system that service has stopped.
    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(20),
        })
        .unwrap();
}

use winapi::um::winnt::PSID;
pub fn sid_to_string(sid: PSID) -> Result<String, DWORD> {
    use widestring::WideString;
    use winapi::shared::minwindef::{BYTE, DWORD, FALSE, HLOCAL, PDWORD};
    use winapi::shared::ntdef::{HANDLE, LPCWSTR, LPWSTR, NULL, WCHAR};
    use winapi::shared::sddl::{ConvertSidToStringSidW, ConvertStringSidToSidW};
    use winapi::shared::winerror::ERROR_SUCCESS;
 
    use winapi::um::aclapi::SetEntriesInAclW;
    use winapi::um::securitybaseapi::{
        AllocateAndInitializeSid, InitializeSecurityDescriptor, SetSecurityDescriptorDacl,
    };
    use winapi::um::winbase::{GetUserNameW, LocalFree, LookupAccountNameW, LookupPrivilegeValueW};
    use winapi::um::winnt::{
        WinNtAuthoritySid, WinWorldSid, ACL, ACL_REVISION, FILE_ALL_ACCESS, GENERIC_READ, GENERIC_WRITE, PACL,
        PSECURITY_DESCRIPTOR, PSID, PSID_IDENTIFIER_AUTHORITY, SECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR_MIN_LENGTH,
        SECURITY_DESCRIPTOR_REVISION, SECURITY_WORLD_RID, SECURITY_WORLD_SID_AUTHORITY,DOMAIN_ALIAS_RID_ADMINS
    };

    let mut raw_string_sid: LPWSTR = NULL as LPWSTR;
    if unsafe { ConvertSidToStringSidW(sid, &mut raw_string_sid) } == 0 || raw_string_sid == (NULL as LPWSTR) {
        return Err(unsafe { GetLastError() });
    }

    let raw_string_sid_len = unsafe { libc::wcslen(raw_string_sid) };
    let sid_string = unsafe { WideString::from_ptr(raw_string_sid, raw_string_sid_len) };

    unsafe { LocalFree(raw_string_sid as HLOCAL) };

    Ok(sid_string.to_string_lossy())
}

unsafe fn create_security_descriptor() -> SECURITY_ATTRIBUTES {
    use winapi::shared::ntdef::{HANDLE, LPCWSTR, LPWSTR, NULL, WCHAR};
    use winapi::shared::winerror::ERROR_SUCCESS;
    use winapi::um::accctrl::{
        EXPLICIT_ACCESS_W, NO_INHERITANCE, NO_MULTIPLE_TRUSTEE, PEXPLICIT_ACCESS_W, SET_ACCESS, TRUSTEE_IS_GROUP,
        TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
    };
    use winapi::um::aclapi::SetEntriesInAclW;
    use winapi::um::securitybaseapi::{
        AllocateAndInitializeSid, InitializeSecurityDescriptor, SetSecurityDescriptorDacl,
    };
    use winapi::um::winnt::{
        WinNtAuthoritySid, WinWorldSid, ACL, ACL_REVISION, FILE_ALL_ACCESS, GENERIC_READ, GENERIC_WRITE, PACL,
        PSECURITY_DESCRIPTOR, PSID, PSID_IDENTIFIER_AUTHORITY, SECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR_MIN_LENGTH,
        SECURITY_DESCRIPTOR_REVISION, SECURITY_WORLD_RID, SECURITY_WORLD_SID_AUTHORITY,SECURITY_BUILTIN_DOMAIN_RID,DOMAIN_ALIAS_RID_ADMINS,SECURITY_NT_AUTHORITY
    };
   
    let SIDAuthWorld = SECURITY_WORLD_SID_AUTHORITY;
    let SIDAuthNT = SECURITY_NT_AUTHORITY;

    let mut pEveryoneSID: PSID = std::mem::zeroed();
    let mut pAdminSID: PSID = std::mem::zeroed();

    let dw = AllocateAndInitializeSid(
        &mut winapi::um::winnt::SID_IDENTIFIER_AUTHORITY { Value: SIDAuthWorld },
        1,
        SECURITY_WORLD_RID,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        &mut pEveryoneSID,
    );
    if dw == FALSE {
        panic!("AllocateAndInitializeSid Error {} {}", GetLastError(), dw);
    }

     if AllocateAndInitializeSid(
             &mut winapi::um::winnt::SID_IDENTIFIER_AUTHORITY { Value: SIDAuthNT },
             2,
            SECURITY_BUILTIN_DOMAIN_RID,
            DOMAIN_ALIAS_RID_ADMINS,
            0, 0, 0, 0, 0, 0,
            &mut pAdminSID) == FALSE
    {
        panic!("AllocateAndInitializeSid Error {} {}", GetLastError(), dw);
    }


     let mut everyone_sid = to_wstring(&sid_to_string(pEveryoneSID).unwrap());
     let mut admin_sid = to_wstring(&sid_to_string(pAdminSID).unwrap());


    println!("pEveryoneSID sid: {:?}", everyone_sid);
    println!("WinWorldSid sid: {:?}", WinNtAuthoritySid);
    println!("admin sid: {:?}", admin_sid);



    let mut ea: [EXPLICIT_ACCESS_W; 2] = [
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                pMultipleTrustee: null_mut(),
                ptstrName: everyone_sid.as_mut_ptr() as LPWSTR,
            },
        },
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_GROUP,
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                pMultipleTrustee: null_mut(),
                ptstrName: admin_sid.as_mut_ptr() as LPWSTR,
            },
        },
    ];

 let mut ea2 = vec![
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                pMultipleTrustee: null_mut(),
                ptstrName: everyone_sid.as_mut_ptr() as LPWSTR,
            },
        },
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_GROUP,
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                pMultipleTrustee: null_mut(),
                ptstrName: admin_sid.as_mut_ptr() as LPWSTR,
            },
        },
    ];


    let mut ea3: [EXPLICIT_ACCESS_W; 2] = std::mem::zeroed();
    ea[0].grfAccessPermissions =  FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ;
    ea[0].grfAccessMode = SET_ACCESS;
    ea[0].grfInheritance= NO_INHERITANCE;
    ea[0].Trustee.TrusteeForm = TRUSTEE_IS_SID;
    ea[0].Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
    ea[0].Trustee.ptstrName  = everyone_sid.as_mut_ptr() as LPWSTR;

    ea[1].grfAccessPermissions = FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ;
    ea[1].grfAccessMode = SET_ACCESS;
    ea[1].grfInheritance= NO_INHERITANCE;
    ea[1].Trustee.TrusteeForm = TRUSTEE_IS_SID;
    ea[1].Trustee.TrusteeType = TRUSTEE_IS_GROUP;
    ea[1].Trustee.ptstrName  = admin_sid.as_mut_ptr() as LPWSTR;

    let mut acl2: ACL = std::mem::zeroed();
    let mut pACL: PACL = &mut acl2;

    println!("ACL: {:?}, {:?}, {:?}, {:?}, {:?}", acl2.AclRevision, acl2.Sbz1, acl2.Sbz2, acl2.AclSize, acl2.AceCount );

    let mut ereyone = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS | GENERIC_WRITE | GENERIC_READ,
        grfAccessMode: SET_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            pMultipleTrustee: NULL as *mut TRUSTEE_W,
            ptstrName: everyone_sid.as_mut_ptr() as LPWSTR,
        },
    };

    let pex: PEXPLICIT_ACCESS_W = &mut ea[0];
    // let dwRes = SetEntriesInAclW(1, pex, NULL as PACL, &mut pACL);
    let dwRes = SetEntriesInAclW(2, ea3.as_mut_ptr() , null_mut(), &mut pACL  );
    // if ERROR_SUCCESS != dwRes {
    //     panic!("Failed to set ACL entries, GLE={} {:?}", GetLastError(), dwRes);
    // }

    // // Initialize a security descriptor.
    // let secDesc = Vec::with_capacity(SECURITY_DESCRIPTOR_MIN_LENGTH);

    //     let secDesc = Vec::with_capacity(SECURITY_DESCRIPTOR_MIN_LENGTH);
    // let pSD =

    let mut secDesc = SECURITY_DESCRIPTOR {
        Revision: 0,
        Sbz1: 0,
        Control: 0, //SECURITY_DESCRIPTOR_CONTROL,
        Owner: null_mut(),
        Group: null_mut(),
        Sacl: null_mut(),
        Dacl: null_mut(),
    };
    let pSD: PSECURITY_DESCRIPTOR = &mut secDesc as *const _ as *mut _;

    // auto secDesc = std::vector<unsigned char>(SECURITY_DESCRIPTOR_MIN_LENGTH);
    // PSECURITY_DESCRIPTOR pSD = (PSECURITY_DESCRIPTOR)(&secDesc[0]);

    if InitializeSecurityDescriptor(pSD, SECURITY_DESCRIPTOR_REVISION) != TRUE {
        panic!("InitializeSecurityDescriptor failed, GLE={}", GetLastError());
    }

    // // Add the ACL to the security descriptor.
    if SetSecurityDescriptorDacl(pSD, TRUE, pACL, FALSE) != TRUE
    // not a default DACL
    {
        panic!("SetSecurityDescriptorDacl failed,  GLE={}", GetLastError());
    }

    // // Initialize a security attributes structure.
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as DWORD,
        lpSecurityDescriptor: pSD,
        bInheritHandle: FALSE,
    };

    return sa;
}

fn run_requests(shutdown_rx: Receiver<()>) {
    info!("Hello, world!");

    let pipe_name = to_wstring("\\\\.\\pipe\\dude");
    let mut lens = lens::Lens::new();
    //    update_lens(&mut lens);

    unsafe {
        let mut sa = create_security_descriptor();

        let h_pipe = CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_BYTE,
            1,           // Max instances
            BUFFER_SIZE, // Out buffer
            BUFFER_SIZE, // In buffer
            0,           // default timeout
            &mut sa,
            // null_mut()
        );

        // match shutdown_rx.recv() {
        //     // Break the loop either upon stop or channel disconnect
        //     Ok(_) => return,
        //     Err(_) => return,
        //     // Err(RecvError::Disconnected) => return,

        //     // // Continue work if no events were received within the timeout
        //     // Err(RecvError::Empty) => (),
        // };

        while h_pipe != INVALID_HANDLE_VALUE {
            info!("Connecting pipe");
            let connected = ConnectNamedPipe(h_pipe, null_mut());
            if connected != FALSE {
                debug!("Connected!");

                let mut buf = [0u8; BUFFER_SIZE as usize];
                let mut dw_read: DWORD = 0;

                while let Some(size) = read_size(h_pipe) {
                    if ReadFile(h_pipe, &mut buf as *mut _ as LPVOID, size, &mut dw_read, null_mut()) != FALSE {
                        trace!(
                            "Read data: {:?} as int: {:?}",
                            dw_read,
                            buf[0..(size as usize)].to_vec()
                        );

                        let _start = PreciseTime::now();

                        let req = parse_request(&buf);
                        let _sent = handle_request(h_pipe, req, &mut lens);

                        let _end = PreciseTime::now();

                        trace!("{} bytes took {:?} ms", _sent, _start.to(_end).num_milliseconds());
                    }
                }
            } else {
                DisconnectNamedPipe(h_pipe);
            }

            info!("Check for service events");
            match shutdown_rx.try_recv() {
                // Break the loop either upon stop or channel disconnect
                Ok(_) => return,
                Err(TryRecvError::Disconnected) => return,

                // Continue work if no events were received within the timeout
                Err(TryRecvError::Empty) => (),
            };
        }
    }

    info!("Farewell, cruel world!");

    std::thread::sleep(Duration::from_secs(2));
}

unsafe fn read_size(pipe_handle: HANDLE) -> Option<u32> {
    let mut size_buf = [0u8; 4];
    let mut dw_read: DWORD = 0;

    info!("Readfile");

    if ReadFile(
        pipe_handle,
        &mut size_buf as *mut _ as LPVOID,
        size_buf.len() as u32,
        &mut dw_read,
        null_mut(),
    ) != FALSE
    {
        let size = to_u32(size_buf);
        return Some(size);
    } else {
        return None;
    }
}

fn send_response(pipe_handle: HANDLE, buf: &[u8]) -> usize {
    let mut dw_write: DWORD = 0;
    let success;
    let size_buf = from_u32(buf.len() as u32);

    unsafe {
        WriteFile(
            pipe_handle,
            size_buf.as_ptr() as LPCVOID,
            size_buf.len() as u32,
            &mut dw_write,
            null_mut(),
        );

        success = WriteFile(
            pipe_handle,
            buf.as_ptr() as LPCVOID,
            buf.len() as u32,
            &mut dw_write,
            null_mut(),
        );
    }

    if success == FALSE {
        warn!("Thingie closed during write?");
    }

    if success == TRUE && dw_write != buf.len() as u32 {
        error!("Write less then buffer!");
        panic!("Write less then buffer!");
    }

    dw_write as usize
}

fn parse_request(buf: &[u8]) -> Request {
    use std::mem::transmute;

    let slice = &buf[0..];
    let mut rdr = Cursor::new(slice);
    let request_type: RequestType =
        num::FromPrimitive::from_u16(rdr.read_u16::<LittleEndian>().unwrap()).expect("Failed to read request type");
    trace!("Got request: {:?}", request_type);

    match request_type {
        RequestType::ReloadStore => Request::Reload,

        RequestType::DirCount => Request::DirCount,
        RequestType::DirRequest => {
            let n1 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize DirRequest");
            Request::DirRequest(n1)
        }

        RequestType::DirFileCount => {
            let n1 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize DirFileCount");
            Request::DirFileCount(n1)
        }
        RequestType::FileRequest => {
            let n1 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize FileRequest start");
            let n2 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize FileRequest end");
            Request::FileRequest(n1, n2)
        }

        RequestType::ChangeSearchText => {
            let mut de = Deserializer::new(&slice[2..]);
            let new_string = Deserialize::deserialize(&mut de).expect("Failed to deserialize ChangeSearchText");
            Request::ChangeSearchText(new_string)
        }

        RequestType::Sort => {
            let sort_column: u32 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize sort_column");
            let sort_order: u32 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize sort_order");

            Request::Sort(
                num::FromPrimitive::from_u32(sort_column).expect("Failed to parse sort_column"),
                num::FromPrimitive::from_u32(sort_order).expect("Failed to parse sort_order"),
            )
        }

        RequestType::LabelAdd => {
            let mut de = Deserializer::new(&slice[2..]);
            let new_string = Deserialize::deserialize(&mut de).expect("Failed to deserialize LabelAdd");
            Request::LabelAdd(new_string)
        }
        RequestType::LabelRemove => {
            let n1 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize LabelRemove");
            Request::LabelRemove(n1)
        }
        RequestType::LabelsGet => Request::LabelsGet,

        RequestType::GetDirLabels => {
            let n1 = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize GetDirLabels");
            Request::GetDirLabels(n1)
        }
        RequestType::AddDirLabels => {
            let entries = read_list(&mut rdr).expect("AddDirLabels(): Failed to read entries list");
            let label_ids = read_list(&mut rdr).expect("AddDirLabels(): Failed to read labels list");

            Request::AddDirLabels(entries, label_ids)
        }
        RequestType::FilterLabel => {
            let label_id = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize FilterLabel label_id");
            let state = rdr.read_u8().expect("Failed to deserialize FilterLabel state");

            Request::FilterLabel(label_id, state)
        }

        RequestType::AddLocation => {
            let raw_s1 = read_byte_list(&mut rdr).unwrap();
            let raw_s2 = read_byte_list(&mut rdr).unwrap();

            let name_string = String::from_utf8(raw_s1).expect("Failed to deserialize AddLocation name_string");
            let path_string = String::from_utf8(raw_s2).expect("Failed to deserialize AddLocation path_string");

            Request::AddLocation(name_string, path_string)
        }
        RequestType::RemoveLocation => {
            let location_id = rdr
                .read_u32::<LittleEndian>()
                .expect("Failed to deserialize RemoveLocation location_id");

            Request::RemoveLocation(location_id)
        }
        RequestType::GetLocations => Request::GetLocations,

        _ => panic!("Unsupported request! {:?}", request_type),
    }
}

fn from_u32(number: u32) -> [u8; 4] {
    unsafe { std::mem::transmute(number) }
}

fn to_u32(number_buf: [u8; 4]) -> u32 {
    unsafe { std::mem::transmute(number_buf) }
}

fn read_list(reader: &mut Cursor<&[u8]>) -> Result<Vec<u32>, std::io::Error> {
    let list_count = reader.read_u32::<LittleEndian>()?;

    let mut list = Vec::new();

    for _ in 0..list_count {
        let id = reader.read_u32::<LittleEndian>()?;
        list.push(id);
    }

    return Ok(list);
}

fn read_byte_list(reader: &mut Cursor<&[u8]>) -> Result<Vec<u8>, std::io::Error> {
    let list_count = reader.read_u32::<LittleEndian>()?;

    let mut list = Vec::new();

    for _ in 0..list_count {
        let id = reader.read_u8()?;
        list.push(id);
    }

    return Ok(list);
}

/***
    Request file:
    tag: u8
    ix: u32
    <tag><ix>
*/

fn handle_request(pipe_handle: HANDLE, req: Request, mut lens: &mut lens::Lens) -> usize {
    trace!("Handling Request");

    match req {
        Request::DirRequest(ix) => handle_dir_request(pipe_handle, &lens, ix),
        Request::FileRequest(dir_ix, file_ix) => handle_file_request(pipe_handle, &lens, dir_ix, file_ix),
        Request::ChangeSearchText(new_search_text) => {
            lens.update_search_text(&new_search_text);
            send_response(pipe_handle, &from_u32(lens.ix_list.len() as u32))
        }
        Request::DirCount => {
            debug!("DirCount {}", lens.get_dir_count() as u32);
            send_response(pipe_handle, &from_u32(lens.get_dir_count() as u32))
        }
        Request::DirFileCount(ix) => {
            let file_count = lens
                .get_file_count(ix as usize)
                .expect(&format!("Invalid index {} during file count", ix)) as u32;
            debug!("FileCount {}", file_count);
            send_response(pipe_handle, &from_u32(file_count))
        }
        Request::Reload => {
            update_lens(&mut lens);
            let mut out_buf = Vec::new();
            out_buf.push(0);
            send_response(pipe_handle, &out_buf)
        }
        Request::DeletePath(_path) => 0,
        Request::Sort(col, order) => {
            debug!("SortRequest: {:?} {:?}", col, order);
            lens.order_by(col, order);
            let r: u32 = 1;
            send_response(pipe_handle, &from_u32(r))
        }

        Request::LabelAdd(name) => {
            debug!("LabelAdd: {:?}", name);
            lens.add_label(&name);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::LabelRemove(id) => {
            debug!("LabelRemove: {:?}", id);
            lens.remove_label(id);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::LabelsGet => {
            debug!("LabelsGet");
            handle_labels_request(pipe_handle, &lens)
        }

        Request::GetDirLabels(entry_id) => {
            debug!("GetDirLabels");
            handle_dir_labels_request(pipe_handle, entry_id, &lens)
        }
        Request::AddDirLabels(entries, label_ids) => {
            debug!(
                "AddDirLabels() Got entry {:?} and labels {:?} ",
                entries.len(),
                label_ids.len()
            );
            lens.set_entry_labels(entries, label_ids);
            send_response(pipe_handle, &from_u32(0))
        }

        Request::FilterLabel(label_id, state) => {
            match state {
                0 => lens.remove_label_filter(label_id),
                1 => lens.add_inlude_label(label_id),
                2 => lens.add_exclude_label(label_id),
                _ => panic!("Ermagad, this state is not supported!"),
            }

            send_response(pipe_handle, &from_u32(0))
        }

        Request::AddLocation(name, path) => {
            lens.add_location(&name, &path);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::RemoveLocation(location_id) => {
            lens.remove_location(location_id);
            send_response(pipe_handle, &from_u32(0))
        }
        Request::GetLocations => handle_locations_request(pipe_handle, &lens), // add update?
    }
}

fn handle_dir_request(pipe_handle: HANDLE, lens: &lens::Lens, ix: u32) -> usize {
    use serious_organizer_lib::models::EntryId;

    let mut out_buf = Vec::new();

    if let Some(dir) = lens.get_dir_entry(ix as usize) {
        let EntryId(entry_id) = dir.id;
        let dir_response = DirEntryResponse {
            id: entry_id,
            name: dir.name.clone(),
            path: dir.path.clone(),
            size: dir.size as u64,
        };
        dir_response
            .serialize(&mut Serializer::new(&mut out_buf))
            .expect("Failed to serialize DirRequest");
        send_response(pipe_handle, &out_buf)
    } else {
        out_buf.push(0xc0);
        send_response(pipe_handle, &out_buf)
    }
}

fn handle_file_request(pipe_handle: HANDLE, lens: &lens::Lens, dir_ix: u32, file_ix: u32) -> usize {
    trace!("FileRequest dir: {} file: {}", dir_ix, file_ix);
    let mut out_buf = Vec::new();

    if let Some(file) = lens.get_file_entry(dir_ix as usize, file_ix as usize) {
        let file_response = FileEntryResponse {
            name: file.name.clone(),
            path: file.path.clone(),
            size: file.size as u64,
        };
        file_response
            .serialize(&mut Serializer::new(&mut out_buf))
            .expect("Failed to serialize FileRequest");
        send_response(pipe_handle, &out_buf)
    } else {
        out_buf.push(0xc0);
        send_response(pipe_handle, &out_buf)
    }
}

fn handle_labels_request(pipe_handle: HANDLE, lens: &lens::Lens) -> usize {
    let mut out_buf = Vec::new();
    lens.get_labels()
        .serialize(&mut Serializer::new(&mut out_buf))
        .expect("Failed to serialize labels request");
    trace!("handle_labels_request bytes: {:?}", out_buf.len());
    send_response(pipe_handle, &out_buf)
}

fn handle_dir_labels_request(pipe_handle: HANDLE, entry_id: u32, lens: &lens::Lens) -> usize {
    let mut out_buf = Vec::new();
    lens.entry_labels(entry_id)
        .serialize(&mut Serializer::new(&mut out_buf))
        .expect("Failed to serialize label for entries");
    trace!("handle_labels_for_entry_request bytes: {:?}", out_buf.len());
    send_response(pipe_handle, &out_buf)
}

fn handle_locations_request(pipe_handle: HANDLE, lens: &lens::Lens) -> usize {
    let mut out_buf = Vec::new();
    lens.get_locations()
        .serialize(&mut Serializer::new(&mut out_buf))
        .expect("Failed to serialize locations request");
    trace!("handle_locations_request bytes: {:?}", out_buf.len());
    send_response(pipe_handle, &out_buf)
}

fn update_lens(lens: &mut lens::Lens) {
    let paths = lens.get_locations().iter().map(|e| (e.id, e.path.clone())).collect();
    let mut dir_s = dir_search::get_all_data(paths);

    lens.update_data(&mut dir_s);
}
