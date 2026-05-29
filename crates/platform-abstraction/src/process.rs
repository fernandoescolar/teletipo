#![allow(unsafe_code)]

use crate::traits::ProcessInfo;

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessInfo;

pub fn current_process_info() -> SystemProcessInfo {
    SystemProcessInfo
}

impl ProcessInfo for SystemProcessInfo {
    fn read_child_cwd(&self, pid: u32) -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            // Use macOS proc_pidinfo(PROC_PIDVNODEPATHINFO) to get the cwd path.
            // Struct sizes (verified against <sys/proc_info.h>):
            //   vinfo_stat       = 136 bytes
            //   vnode_info       = vinfo_stat(136) + vi_type(4) + vi_pad(4) + vi_fsid(8) = 152 bytes
            //   vnode_info_path  = vnode_info(152) + path[MAXPATHLEN=1024] = 1176 bytes
            //   proc_vnodepathinfo = pvi_cdir(1176) + pvi_rdir(1176) = 2352 bytes
            const PROC_PIDVNODEPATHINFO: i32 = 9;
            const BUF_SIZE: usize = 2352;
            const PATH_OFFSET: usize = 152; // sizeof(vnode_info)
            const MAXPATHLEN: usize = 1024;
            unsafe extern "C" {
                unsafe fn proc_pidinfo(
                    pid: i32,
                    flavor: i32,
                    arg: u64,
                    buffer: *mut u8,
                    buffersize: i32,
                ) -> i32;
            }
            let mut buf = vec![0u8; BUF_SIZE];
            let ret = unsafe {
                proc_pidinfo(
                    pid as i32,
                    PROC_PIDVNODEPATHINFO,
                    0,
                    buf.as_mut_ptr(),
                    BUF_SIZE as i32,
                )
            };
            if ret <= 0 {
                return None;
            }
            let path_bytes = &buf[PATH_OFFSET..PATH_OFFSET + MAXPATHLEN];
            let end = path_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(MAXPATHLEN);
            String::from_utf8(path_bytes[..end].to_vec())
                .ok()
                .filter(|s| !s.is_empty())
        }

        #[cfg(not(target_os = "macos"))]
        {
            // On Linux read the cwd symlink from procfs.
            std::fs::read_link(format!("/proc/{}/cwd", pid))
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        }
    }
}
