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
        ConfigPath = ResolveConfigPath();
    }

    /// <summary>配置保存到 %AppData%\MC-Tunnel，换目录/更新版也能记住</summary>
    public static string ResolveConfigPath()
    {
        var appDataDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "MC-Tunnel");
        Directory.CreateDirectory(appDataDir);
        var target = Path.Combine(appDataDir, "mc-tunnel-config.json");

        if (File.Exists(target))
            return target;

        foreach (var legacy in LegacyConfigPaths())
        {
            if (!File.Exists(legacy))
                continue;
            try
            {
                File.Copy(legacy, target, overwrite: false);
                return target;
            }
            catch
            {
                return legacy;
            }
        }

        return target;
    }

    private static IEnumerable<string> LegacyConfigPaths()
    {
        yield return Path.Combine(TunnelExeLocator.ConfigDirectory(), "mc-tunnel-config.json");
        var downloads = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            "Downloads");
        yield return Path.Combine(downloads, "MC-Tunnel-Windows-x64", "mc-tunnel-config.json");
        yield return Path.Combine(downloads, "MC-Tunnel-Windows-x64 (1)", "mc-tunnel-config.json");
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
        "fast" => SpeedMode.Fast,
        _ => SpeedMode.Turbo,
    };

    public static string SpeedToCli(SpeedMode speed) => speed switch
    {
        SpeedMode.Balanced => "balanced",
        SpeedMode.Stealth => "stealth",
        SpeedMode.Fast => "fast",
        _ => "turbo",
    };

    public static string ProxyModeToCli(ProxyMode mode) => mode switch
    {
        ProxyMode.All => "all",
        ProxyMode.Direct => "direct",
        _ => "gfw",
    };
}
