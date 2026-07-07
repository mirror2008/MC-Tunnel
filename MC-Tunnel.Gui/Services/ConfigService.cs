using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using MCTunnel.Gui.Models;

namespace MCTunnel.Gui.Services;

public sealed class ConfigService
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        WriteIndented = true,
        PropertyNamingPolicy = null,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower) },
    };

    public string ConfigPath { get; }

    public ConfigService()
    {
        ConfigPath = Path.Combine(TunnelExeLocator.ConfigDirectory(), "mc-tunnel-config.json");
    }

    public ClientConfig Load()
    {
        if (!File.Exists(ConfigPath))
            return new ClientConfig();

        try
        {
            var json = File.ReadAllText(ConfigPath);
            return JsonSerializer.Deserialize<ClientConfig>(json, JsonOptions) ?? new ClientConfig();
        }
        catch
        {
            return new ClientConfig();
        }
    }

    public void Save(ClientConfig config)
    {
        var dir = Path.GetDirectoryName(ConfigPath);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);

        var json = JsonSerializer.Serialize(config, JsonOptions);
        File.WriteAllText(ConfigPath, json);
    }

    public static string NormalizeDomain(string domain)
    {
        domain = domain.Trim().TrimStart('.').TrimEnd('.').ToLowerInvariant();
        return domain;
    }

    public static SpeedMode ParseSpeed(string speed) => speed switch
    {
        "balanced" => SpeedMode.Balanced,
        "stealth" => SpeedMode.Stealth,
        _ => SpeedMode.Fast,
    };

    public static string SpeedToCli(SpeedMode speed) => speed switch
    {
        SpeedMode.Balanced => "balanced",
        SpeedMode.Stealth => "stealth",
        _ => "fast",
    };

    public static string ProxyModeToCli(ProxyMode mode) => mode switch
    {
        ProxyMode.All => "all",
        ProxyMode.Direct => "direct",
        _ => "gfw",
    };
}
