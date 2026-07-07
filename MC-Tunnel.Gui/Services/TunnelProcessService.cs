using System.Diagnostics;
using System.IO;
using System.Net.Sockets;
using System.Text;
using MCTunnel.Gui.Models;

namespace MCTunnel.Gui.Services;

public sealed class TunnelProcessService : IDisposable
{
    private Process? _process;
    private readonly object _gate = new();
    private int _stopping;

    public event Action<string>? LogReceived;
    public event Action? ProcessExited;

    public string TunnelExePath { get; }

    public TunnelProcessService()
    {
        TunnelExePath = TunnelExeLocator.FindTunnelExe();
    }

    public bool IsRunning
    {
        get
        {
            lock (_gate)
            {
                return _process is { HasExited: false };
            }
        }
    }

    public void CleanupStaleProcesses()
    {
        try
        {
            if (File.Exists(TunnelExePath))
                RunSilent(TunnelExePath, "stop", 3000);
        }
        catch { /* ignore */ }

        KillByName("mc-tunnel");
        Thread.Sleep(300);
    }

    public bool Start(ClientConfig config, string configPath)
    {
        lock (_gate)
        {
            if (_process is { HasExited: false })
                return false;

            if (!File.Exists(TunnelExePath))
            {
                EmitLog($"找不到 mc-tunnel.exe: {TunnelExePath}");
                return false;
            }

            CleanupStaleProcesses();
            Interlocked.Exchange(ref _stopping, 0);

            var args = new StringBuilder();
            args.Append("client ");
            args.Append($"--remote \"{config.Remote}\" ");
            args.Append($"--local \"{config.LocalProxy}\" ");
            args.Append($"--speed {ConfigService.SpeedToCli(ConfigService.ParseSpeed(config.Speed))} ");
            args.Append($"--proxy-mode {ConfigService.ProxyModeToCli(config.ProxyMode)} ");
            args.Append($"--config \"{configPath}\" ");
            args.Append(config.SetSystemProxy ? "--set-proxy true" : "--set-proxy false");

            var psi = new ProcessStartInfo
            {
                FileName = TunnelExePath,
                Arguments = args.ToString(),
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8,
            };

            var process = new Process { StartInfo = psi, EnableRaisingEvents = true };
            DataReceivedEventHandler onOut = (_, e) =>
            {
                if (!string.IsNullOrEmpty(e.Data))
                    EmitLog(e.Data);
            };
            DataReceivedEventHandler onErr = (_, e) =>
            {
                if (!string.IsNullOrEmpty(e.Data))
                    EmitLog($"[错误] {e.Data}");
            };
            EventHandler onExit = (_, _) => ProcessExited?.Invoke();

            process.OutputDataReceived += onOut;
            process.ErrorDataReceived += onErr;
            process.Exited += onExit;

            if (!process.Start())
                return false;

            _process = process;
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
            return true;
        }
    }

    /// <summary>非阻塞断开：立即返回，实际清理在后台线程完成。</summary>
    public void Stop()
    {
        if (Interlocked.Exchange(ref _stopping, 1) == 1)
            return;

        Process? proc;
        lock (_gate)
        {
            proc = _process;
            _process = null;
        }

        _ = Task.Run(() => StopProcessCore(proc));
    }

    private void StopProcessCore(Process? proc)
    {
        try
        {
            if (proc is not null)
            {
                try { proc.EnableRaisingEvents = false; } catch { /* ignore */ }
                TerminateProcess(proc);
            }

            if (File.Exists(TunnelExePath))
                RunSilent(TunnelExePath, "stop", 2500);
        }
        catch { /* ignore */ }
        finally
        {
            Interlocked.Exchange(ref _stopping, 0);
        }
    }

    private static void TerminateProcess(Process proc)
    {
        try
        {
            proc.EnableRaisingEvents = false;

            if (!proc.HasExited)
            {
                // 勿杀 entireProcessTree：会连带 javaw 雷神中继，极易卡死
                proc.Kill(entireProcessTree: false);
                proc.WaitForExit(2000);
            }
        }
        catch { /* ignore */ }
        finally
        {
            try { proc.Dispose(); } catch { /* ignore */ }
        }
    }

    public static bool IsProxyPortOpen() => TryConnect("127.0.0.1", 8080);

    private static bool TryConnect(string host, int port)
    {
        try
        {
            using var client = new TcpClient { NoDelay = true };
            using var cts = new CancellationTokenSource(200);
            var connectTask = client.ConnectAsync(host, port, cts.Token).AsTask();
            return connectTask.Wait(250) && client.Connected;
        }
        catch
        {
            return false;
        }
    }

    private static void KillByName(string name)
    {
        foreach (var p in Process.GetProcessesByName(name))
        {
            try { p.Kill(entireProcessTree: false); } catch { /* ignore */ }
            finally { p.Dispose(); }
        }
    }

    private static void RunSilent(string file, string args, int waitMs)
    {
        try
        {
            using var p = Process.Start(new ProcessStartInfo
            {
                FileName = file,
                Arguments = args,
                UseShellExecute = false,
                CreateNoWindow = true,
            });
            p?.WaitForExit(waitMs);
        }
        catch { /* ignore */ }
    }

    private void EmitLog(string line) => LogReceived?.Invoke(line);

    public void Dispose() => Stop();
}
