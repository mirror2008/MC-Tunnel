//! 域名分流规则（GeoIP 不可用时的回退，以及 PAC 生成）

/// 精确匹配的域名（小写）
const GFW_EXACT: &[&str] = &[
    "google.com",
    "youtube.com",
    "facebook.com",
    "twitter.com",
    "x.com",
    "instagram.com",
    "telegram.org",
    "discord.com",
    "reddit.com",
    "wikipedia.org",
    "github.com",
    "gitlab.com",
    "openai.com",
    "chatgpt.com",
    "medium.com",
    "twitch.tv",
    "whatsapp.com",
    "dropbox.com",
    "spotify.com",
    "netflix.com",
    "bbc.com",
    "nytimes.com",
    "archive.org",
    "pinterest.com",
    "linkedin.com",
    "t.me",
    "line.me",
    "snapchat.com",
    "vimeo.com",
    "dailymotion.com",
    "soundcloud.com",
    "bandcamp.com",
    "pixiv.net",
    "nicovideo.jp",
    "google.com.hk",
];

/// 后缀匹配（含点）
const GFW_SUFFIXES: &[&str] = &[
    ".google.com",
    ".googleapis.com",
    ".gstatic.com",
    ".googlevideo.com",
    ".googleusercontent.com",
    ".ggpht.com",
    ".gvt1.com",
    ".youtube.com",
    ".ytimg.com",
    ".youtu.be",
    ".facebook.com",
    ".fbcdn.net",
    ".instagram.com",
    ".cdninstagram.com",
    ".twitter.com",
    ".twimg.com",
    ".x.com",
    ".telegram.org",
    ".t.me",
    ".discord.com",
    ".discordapp.com",
    ".discord.gg",
    ".reddit.com",
    ".redd.it",
    ".redditmedia.com",
    ".wikipedia.org",
    ".wikimedia.org",
    ".github.com",
    ".githubusercontent.com",
    ".gitlab.com",
    ".openai.com",
    ".chatgpt.com",
    ".oaistatic.com",
    ".oaiusercontent.com",
    ".medium.com",
    ".twitch.tv",
    ".ttvnw.net",
    ".whatsapp.com",
    ".whatsapp.net",
    ".dropbox.com",
    ".dropboxusercontent.com",
    ".spotify.com",
    ".scdn.co",
    ".netflix.com",
    ".nflxvideo.net",
    ".bbc.com",
    ".bbc.co.uk",
    ".nytimes.com",
    ".archive.org",
    ".pinterest.com",
    ".pinimg.com",
    ".linkedin.com",
    ".licdn.com",
    ".line.me",
    ".line-apps.com",
    ".snapchat.com",
    ".vimeo.com",
    ".vimeocdn.com",
    ".dailymotion.com",
    ".soundcloud.com",
    ".bandcamp.com",
    ".pixiv.net",
    ".pximg.net",
    ".nicovideo.jp",
    ".bing.com",
    ".live.com",
    ".microsoft.com",
    ".office.com",
    ".office365.com",
    ".skype.com",
    ".zoom.us",
    ".slack.com",
    ".quora.com",
    ".tumblr.com",
    ".blogspot.com",
    ".blogspot.hk",
    ".appspot.com",
    ".android.com",
    ".chrome.com",
    ".chromium.org",
    ".gmail.com",
    ".googlemail.com",
    ".duckduckgo.com",
    ".proton.me",
    ".protonmail.com",
    ".mega.nz",
    ".mega.io",
    ".torproject.org",
    ".wikileaks.org",
    ".apkmirror.com",
    ".apkpure.com",
    ".steamcommunity.com",
    ".steampowered.com",
    ".steamstatic.com",
    ".epicgames.com",
    ".curseforge.com",
    ".modrinth.com",
    ".fabricmc.net",
];

/// 国内常见域名后缀，明确不走代理
const CHINA_SUFFIXES: &[&str] = &[
    ".cn",
    ".中国",
    ".com.cn",
    ".net.cn",
    ".org.cn",
    ".gov.cn",
    ".edu.cn",
    ".baidu.com",
    ".qq.com",
    ".weixin.qq.com",
    ".taobao.com",
    ".tmall.com",
    ".alipay.com",
    ".alicdn.com",
    ".jd.com",
    ".163.com",
    ".126.com",
    ".bilibili.com",
    ".bilivideo.com",
    ".hdslb.com",
    ".zhihu.com",
    ".douyin.com",
    ".tiktokv.com",
    ".weibo.com",
    ".sina.com",
    ".sohu.com",
    ".iqiyi.com",
    ".youku.com",
    ".tencent.com",
    ".mi.com",
    ".xiaomi.com",
    ".huawei.com",
    ".csdn.net",
    ".douban.com",
    ".meituan.com",
    ".ele.me",
    ".aliyun.com",
    ".toutiao.com",
    ".snssdk.com",
    ".360.cn",
    ".360.com",
    ".sogou.com",
    ".sm.cn",
    ".12306.cn",
    ".gov.cn",
];

pub fn host_from_dest(dest: &str) -> &str {
    dest.rsplit_once(':').map(|(h, _)| h).unwrap_or(dest)
}

pub fn is_china_site(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    CHINA_SUFFIXES.iter().any(|suf| host_matches_suffix(&host, suf))
}

/// GeoIP 不可用时的域名回退判定
pub fn needs_proxy_by_domain(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host == "localhost" || host.ends_with(".local") {
        return false;
    }
    if is_china_site(&host) {
        return false;
    }
    if GFW_EXACT.iter().any(|d| *d == host) {
        return true;
    }
    GFW_SUFFIXES.iter().any(|suf| host_matches_suffix(&host, suf))
}

fn host_matches_suffix(host: &str, suf: &str) -> bool {
    if let Some(stripped) = suf.strip_prefix('.') {
        host.ends_with(suf) || host == stripped
    } else {
        host.ends_with(suf) || host == suf
    }
}

pub fn china_suffix_domains() -> &'static [&'static str] {
    CHINA_SUFFIXES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cn_site() {
        assert!(is_china_site("baidu.com"));
        assert!(!needs_proxy_by_domain("baidu.com"));
    }

    #[test]
    fn gfw_domain() {
        assert!(needs_proxy_by_domain("www.google.com"));
        assert!(!needs_proxy_by_domain("example.de"));
    }
}
