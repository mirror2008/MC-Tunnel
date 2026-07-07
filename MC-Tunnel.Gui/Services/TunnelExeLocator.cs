using System.IO;

namespace MCTunnel.Gui.Services;

public static class TunnelExeLocator
{
    public static string FindTunnelExe()
    {
        var candidates = new List<string>();
        var dir = AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);

        for (var i = 0; i < 8 && !string.IsNullOrEmpty(dir); i++)
        {
            candidates.Add(Path.Combine(dir, "mc-tunnel.exe"));
            candidates.Add(Path.Combine(dir, "release", "mc-tunnel.exe"));
            dir = Directory.GetParent(dir)?.FullName;
        }

        foreach (var path in candidates.Distinct(StringComparer.OrdinalIgnoreCase))
        {
            if (File.Exists(path))
                return path;
        }

        return Path.Combine(AppContext.BaseDirectory, "mc-tunnel.exe");
    }

    public static string ConfigDirectory()
    {
        var tunnel = FindTunnelExe();
        var dir = Path.GetDirectoryName(tunnel);
        return string.IsNullOrEmpty(dir) ? AppContext.BaseDirectory : dir;
    }
}
