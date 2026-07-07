#[cfg(windows)]
mod imp {
    use anyhow::Context;
    use winreg::enums::*;
    use winreg::RegKey;

    const PROXY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    /// 开启 GFW 分流 PAC（仅被墙站点走代理）
    pub fn enable_gfw_pac(pac_file_url: &str) -> anyhow::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let settings = hkcu
            .open_subkey_with_flags(PROXY_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE)
            .context("打开注册表代理设置失败")?;
        settings.set_value("ProxyEnable", &0u32)?;
        settings.set_value("AutoConfigURL", &pac_file_url.to_string())?;
        settings.set_value("ProxyOverride", &"<local>".to_string())?;
        notify_proxy_change();
        tracing::info!("系统 PAC 已启用: {pac_file_url}");
        Ok(())
    }

    /// 开启全局 HTTP 代理（备用）
    pub fn enable_system_proxy(host: &str, port: u16) -> anyhow::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let settings = hkcu
            .open_subkey_with_flags(PROXY_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE)
            .context("打开注册表代理设置失败")?;
        let proxy_server = format!("http={host}:{port};https={host}:{port}");
        settings.set_value("ProxyEnable", &1u32)?;
        settings.set_value("ProxyServer", &proxy_server)?;
        settings.set_value("ProxyOverride", &"<local>".to_string())?;
        settings.delete_value("AutoConfigURL").ok();
        notify_proxy_change();
        tracing::info!("系统 HTTP 代理已开启: {proxy_server}");
        Ok(())
    }

    pub fn disable_system_proxy() -> anyhow::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let settings = hkcu
            .open_subkey_with_flags(PROXY_KEY, KEY_SET_VALUE)
            .context("打开注册表代理设置失败")?;
        settings.set_value("ProxyEnable", &0u32)?;
        settings.delete_value("AutoConfigURL").ok();
        notify_proxy_change();
        tracing::info!("系统代理已关闭");
        Ok(())
    }

    fn notify_proxy_change() {
        #[link(name = "wininet")]
        extern "system" {
            fn InternetSetOptionW(
                h_internet: *mut std::ffi::c_void,
                dw_option: u32,
                lp_buffer: *mut std::ffi::c_void,
                dw_buffer_length: u32,
            ) -> i32;
        }
        const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;
        const INTERNET_OPTION_REFRESH: u32 = 37;
        unsafe {
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_SETTINGS_CHANGED,
                std::ptr::null_mut(),
                0,
            );
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_REFRESH,
                std::ptr::null_mut(),
                0,
            );
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn enable_gfw_pac(_pac_file_url: &str) -> anyhow::Result<()> {
        Ok(())
    }
    pub fn enable_system_proxy(_host: &str, _port: u16) -> anyhow::Result<()> {
        Ok(())
    }
    pub fn disable_system_proxy() -> anyhow::Result<()> {
        Ok(())
    }
}

pub use imp::{disable_system_proxy, enable_gfw_pac, enable_system_proxy};
