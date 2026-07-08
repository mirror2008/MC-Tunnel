using System.Net.Http;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace MCTunnel.Gui.Services;

public sealed class TrafficStatsDto
{
    [JsonPropertyName("up_bps")]
    public long UpBps { get; set; }

    [JsonPropertyName("down_bps")]
    public long DownBps { get; set; }

    [JsonPropertyName("up_total")]
    public long UpTotal { get; set; }

    [JsonPropertyName("down_total")]
    public long DownTotal { get; set; }

    [JsonPropertyName("tunnel_alive")]
    public bool TunnelAlive { get; set; } = true;
}

public sealed class TrafficStatsService : IDisposable
{
    private static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(2) };
    private static readonly Uri StatsUri = new("http://127.0.0.1:8092/stats");

    public async Task<TrafficStatsDto?> FetchAsync(CancellationToken ct = default)
    {
        try
        {
            var json = await Http.GetStringAsync(StatsUri, ct).ConfigureAwait(false);
            return JsonSerializer.Deserialize<TrafficStatsDto>(json);
        }
        catch
        {
            return null;
        }
    }

    public static string FormatSpeed(long bytesPerSec) => bytesPerSec switch
    {
        < 1024 => $"{bytesPerSec} B/s",
        < 1024 * 1024 => $"{bytesPerSec / 1024.0:F1} KB/s",
        _ => $"{bytesPerSec / 1024.0 / 1024.0:F2} MB/s",
    };

    public static string FormatTotal(long bytes) => bytes switch
    {
        < 1024 => $"{bytes} B",
        < 1024 * 1024 => $"{bytes / 1024.0:F1} KB",
        < 1024L * 1024 * 1024 => $"{bytes / 1024.0 / 1024.0:F2} MB",
        _ => $"{bytes / 1024.0 / 1024.0 / 1024.0:F2} GB",
    };

    public void Dispose() { }
}
