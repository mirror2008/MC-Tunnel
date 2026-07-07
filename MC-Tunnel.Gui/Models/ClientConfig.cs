using System.Text.Json.Serialization;

namespace MCTunnel.Gui.Models;

public sealed class ClientConfig
{
    [JsonPropertyName("remote")]
    public string Remote { get; set; } = "";

    [JsonPropertyName("local_proxy")]
    public string LocalProxy { get; set; } = "127.0.0.1:1080";

    [JsonPropertyName("speed")]
    public string Speed { get; set; } = "fast";

    [JsonPropertyName("proxy_mode")]
    public ProxyMode ProxyMode { get; set; } = ProxyMode.GfwOnly;

    [JsonPropertyName("set_system_proxy")]
    public bool SetSystemProxy { get; set; } = true;

    [JsonPropertyName("whitelist")]
    public List<WhitelistEntry> Whitelist { get; set; } = [];
}

public sealed class WhitelistEntry
{
    [JsonPropertyName("domain")]
    public string Domain { get; set; } = "";

    [JsonPropertyName("include_subdomains")]
    public bool IncludeSubdomains { get; set; }
}

[JsonConverter(typeof(JsonStringEnumConverter))]
public enum ProxyMode
{
    All,
    GfwOnly,
    Direct,
}

public enum SpeedMode
{
    Fast,
    Balanced,
    Stealth,
}

public enum ConnectionStatus
{
    Disconnected,
    Connecting,
    Connected,
}
