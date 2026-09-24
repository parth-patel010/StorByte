use anyhow::Result;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{GetSecurityDescriptorLength, PSECURITY_DESCRIPTOR};

const SDDL_REVISION_1: u32 = 1;

pub fn default_security_descriptor() -> Result<Vec<u8>> {
    read_sddl("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)")
}

fn read_sddl(sddl: &str) -> Result<Vec<u8>> {
    let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let mut size = 0u32;
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(wide.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            Some(&mut size),
        )?;
    }
    if descriptor.0.is_null() {
        anyhow::bail!("SDDL parsing returned a null descriptor");
    }
    let len = unsafe { GetSecurityDescriptorLength(descriptor) } as usize;
    let bytes = unsafe { std::slice::from_raw_parts(descriptor.0 as *const u8, len) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    Ok(bytes)
}
