//! 令牌怎么存：先用系统自带的加密（Windows DPAPI）包一层，再落盘。
//!
//! 为什么不自己写 AES：自己发明的加密十有八九是错的（缺认证、缺随机数管理），
//! 而 DPAPI 是系统给的、绑当前用户账号的，换台机器或换个账号就解不开——
//! 正好符合「令牌只属于这台电脑这一个人」。
//!
//! 磁盘上的格式：`dpapi1:<十六进制>`。前缀带版本号，将来换算法时能看懂老文件。
//! 解不开时不算失败：直接当成「没有令牌」，让用户重新粘一次即可。

/// 磁盘文件开头的前缀。
const PREFIX: &str = "dpapi1:";

/// 把明文令牌封成能落盘的一行字。
pub fn seal(plain: &str) -> Result<String, String> {
    let bytes = win::protect(plain.as_bytes())?;
    Ok(format!("{PREFIX}{}", to_hex(&bytes)))
}

/// 把磁盘上的那一行字解回明文令牌。
pub fn open(sealed: &str) -> Result<String, String> {
    let Some(hex) = sealed.trim().strip_prefix(PREFIX) else {
        return Err("这份令牌是用老办法存的，认不出来".to_owned());
    };
    let bytes = from_hex(hex)?;
    let plain = win::unprotect(&bytes)?;
    String::from_utf8(plain).map_err(|_| "解出来的不是一段文字".to_owned())
}

/// 看起来像不像我们封过的令牌（用来区分老文件）。
pub fn looks_sealed(text: &str) -> bool {
    text.trim().starts_with(PREFIX)
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.len() % 2 != 0 {
        return Err("长度不对，不是我们写的格式".to_owned());
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16).ok_or("不是十六进制")?;
        let lo = (pair[1] as char).to_digit(16).ok_or("不是十六进制")?;
        out.push((hi * 16 + lo) as u8);
    }
    Ok(out)
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    /// Windows 的 DATA_BLOB：一块内存 + 它的长度。
    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    /// 不允许弹任何界面：程序可能在没人看着的情况下跑。
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *const c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *const c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    fn run(input: &[u8], protect: bool) -> Result<Vec<u8>, String> {
        if input.is_empty() {
            return Err("要加密的内容是空的".to_owned());
        }
        let in_blob = DataBlob {
            cb_data: input.len() as u32,
            pb_data: input.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };
        // SAFETY: 两个 blob 都指向本函数里活着的内存；成功时系统分配 out_blob.pb_data，
        // 由下面的 LocalFree 释放，失败时按约定不会分配。
        let ok = unsafe {
            if protect {
                CryptProtectData(
                    &in_blob,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out_blob,
                )
            } else {
                CryptUnprotectData(
                    &in_blob,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out_blob,
                )
            }
        };
        if ok == 0 {
            return Err("系统不让这块数据通过：可能换过电脑或换过账号".to_owned());
        }

        let bytes = unsafe {
            let len = out_blob.cb_data as usize;
            let mut buf = Vec::with_capacity(len);
            if !out_blob.pb_data.is_null() && len > 0 {
                buf.extend_from_slice(std::slice::from_raw_parts(out_blob.pb_data, len));
            }
            if !out_blob.pb_data.is_null() {
                LocalFree(out_blob.pb_data as *mut c_void);
            }
            buf
        };
        if bytes.is_empty() {
            return Err("系统返回了空内容".to_owned());
        }
        Ok(bytes)
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        run(plain, true)
    }

    pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, String> {
        run(sealed, false)
    }
}

#[cfg(not(windows))]
mod win {
    pub fn protect(_plain: &[u8]) -> Result<Vec<u8>, String> {
        Err("这个系统上没有系统自带的加密".to_owned())
    }

    pub fn unprotect(_sealed: &[u8]) -> Result<Vec<u8>, String> {
        Err("这个系统上没有系统自带的加密".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 封起来再解回来还是原来那串字() {
        let plain = "0123456789abcdef0123456789abcdef";
        let sealed = seal(plain).expect("封得起来");
        assert!(sealed.starts_with(PREFIX), "要带版本前缀：{sealed}");
        assert!(!sealed.contains(plain), "磁盘上不能出现明文");
        assert_eq!(open(&sealed).expect("解得开"), plain);
    }

    #[test]
    fn 老办法存的令牌认不出来但也不炸() {
        let err = open("0123456789abcdef").unwrap_err();
        assert!(err.contains("老办法"), "{err}");
        assert!(!looks_sealed("0123456789abcdef"));
        assert!(looks_sealed(&format!("{PREFIX}00")));
    }

    #[test]
    fn 改坏一个字符就解不开() {
        let sealed = seal("secret").expect("封得起来");
        let body = sealed.strip_prefix(PREFIX).unwrap().to_owned();
        let mut bytes = from_hex(&body).expect("是十六进制");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        let broken = format!("{PREFIX}{}", to_hex(&bytes));
        assert!(open(&broken).is_err(), "改坏了还得解出原文就不对劲了");
    }

    #[test]
    fn 十六进制两位一字节进出对称() {
        let bytes = [0u8, 15, 16, 255, 128];
        assert_eq!(to_hex(&bytes), "000f10ff80");
        assert_eq!(from_hex("000f10ff80").unwrap(), bytes);
        assert!(from_hex("00f").is_err());
        assert!(from_hex("zz").is_err());
    }
}
