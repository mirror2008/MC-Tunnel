using System.Collections.ObjectModel;
using System.IO;
using System.Windows.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using MCTunnel.Gui.Models;
using MCTunnel.Gui.Services;

namespace MCTunnel.Gui.ViewModels;

public partial class MainViewModel : ObservableObject, IDisposable
{
    private readonly ConfigService _configService = new();
    private readonly TunnelProcessService _tunnel = new();
    private readonly TrafficStatsService _trafficStats = new();
    private readonly DispatcherTimer _pollTimer;
    private readonly List<string> _logBuffer = [];

    [ObservableProperty] private string _remote = "";
    [ObservableProperty] private string _localProxy = "";
    [ObservableProperty] private SpeedMode _speed = SpeedMode.Fast;
    [ObservableProperty] private ProxyMode _proxyMode = ProxyMode.GfwOnly;
    [ObservableProperty] private bool _setSystemProxy = true;
    [ObservableProperty] private ConnectionStatus _status = ConnectionStatus.Disconnected;
    [ObservableProperty] private string _newWhitelistDomain = "";
    [ObservableProperty] private bool _newWhitelistSubdomains = true;
    [ObservableProperty] private string _statusHint = "";
    [ObservableProperty] private string _modeDescription = "";
    [ObservableProperty] private string _uploadSpeed = "0 B/s";
    [ObservableProperty] private string _downloadSpeed = "0 B/s";
    [ObservableProperty] private string _uploadTotal = "0 B";
    [ObservableProperty] private string _downloadTotal = "0 B";

    public ObservableCollection<WhitelistItemViewModel> Whitelist { get; } = [];
    public ObservableCollection<string> Logs { get; } = [];

    public bool IsConnected => Status == ConnectionStatus.Connected;
    public bool IsConnecting => Status == ConnectionStatus.Connecting;
    public bool IsDisconnected => Status == ConnectionStatus.Disconnected;
    public bool ShowWhitelist => ProxyMode == ProxyMode.GfwOnly;
    public bool ShowTraffic => Status == ConnectionStatus.Connected;
    public bool CanEdit => Status != ConnectionStatus.Connecting;

    public string ConnectButtonText => Status switch
    {
        ConnectionStatus.Connected => "断开连接",
        ConnectionStatus.Connecting => "连接中…",
        _ => "连接服务器",
    };

    public bool IsConnectButtonEnabled => Status != ConnectionStatus.Connecting;

    public bool UseDangerButton => Status == ConnectionStatus.Connected;

    public MainViewModel()
    {
        _tunnel.LogReceived += OnLog;
        _tunnel.ProcessExited += OnProcessExited;

        _pollTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(450) };
        _pollTimer.Tick += (_, _) => PollState();
        _pollTimer.Start();

        LoadConfig();
        var tunnel = TunnelExeLocator.FindTunnelExe();
        if (File.Exists(tunnel))
            AddLog($"后端: {tunnel}");
        else
            AddLog($"警告: 找不到 mc-tunnel.exe，请将 GUI 放在 release 目录运行");
        AddLog("就绪。请先启动雷神 → 加速 Minecraft，再点连接");
        AddLog("隧道强制经 javaw/雷神通道，禁止直连 VPS");
        UpdateModeDescription();
    }

    partial void OnProxyModeChanged(ProxyMode value)
    {
        UpdateModeDescription();
        OnPropertyChanged(nameof(ShowWhitelist));
        PersistConfigQuietly();
    }

    partial void OnRemoteChanged(string value) => PersistConfigQuietly();
    partial void OnLocalProxyChanged(string value) => PersistConfigQuietly();
    partial void OnSpeedChanged(SpeedMode value) => PersistConfigQuietly();
    partial void OnSetSystemProxyChanged(bool value) => PersistConfigQuietly();

    partial void OnStatusChanged(ConnectionStatus value)
    {
        OnPropertyChanged(nameof(IsConnected));
        OnPropertyChanged(nameof(IsConnecting));
        OnPropertyChanged(nameof(IsDisconnected));
        OnPropertyChanged(nameof(CanEdit));
        OnPropertyChanged(nameof(ConnectButtonText));
        OnPropertyChanged(nameof(IsConnectButtonEnabled));
        OnPropertyChanged(nameof(UseDangerButton));
        OnPropertyChanged(nameof(ShowTraffic));
        if (value != ConnectionStatus.Connected)
            ResetTrafficDisplay();
        ConnectCommand.NotifyCanExecuteChanged();
        DisconnectCommand.NotifyCanExecuteChanged();
    }

    [RelayCommand(CanExecute = nameof(CanPressConnect))]
    private void ToggleConnect()
    {
        if (Status == ConnectionStatus.Connected)
            Disconnect();
        else
            Connect();
    }

    private bool CanPressConnect() => Status != ConnectionStatus.Connecting;

    private void LoadConfig()
    {
        var cfg = _configService.Load();
        Remote = cfg.Remote;
        LocalProxy = cfg.LocalProxy;
        Speed = ConfigService.ParseSpeed(cfg.Speed);
        ProxyMode = cfg.ProxyMode;
        SetSystemProxy = cfg.SetSystemProxy;

        Whitelist.Clear();
        foreach (var e in cfg.Whitelist)
        {
            Whitelist.Add(new WhitelistItemViewModel
            {
                Domain = e.Domain,
                IncludeSubdomains = e.IncludeSubdomains,
            });
        }
    }

    private ClientConfig BuildConfig() => new()
    {
        Remote = Remote.Trim(),
        LocalProxy = LocalProxy.Trim(),
        Speed = ConfigService.SpeedToCli(Speed),
        ProxyMode = ProxyMode,
        SetSystemProxy = SetSystemProxy,
        Whitelist = Whitelist.Select(w => new WhitelistEntry
        {
            Domain = w.Domain,
            IncludeSubdomains = w.IncludeSubdomains,
        }).ToList(),
    };

    private void SaveConfig()
    {
        try
        {
            _configService.Save(BuildConfig());
        }
        catch (Exception ex)
        {
            AddLog($"保存配置失败: {ex.Message}");
        }
    }

    private void PersistConfigQuietly()
    {
        if (Status == ConnectionStatus.Connecting)
            return;
        try
        {
            _configService.Save(BuildConfig());
        }
        catch
        {
            /* 静默保存，避免输入时刷屏 */
        }
    }

    private void UpdateModeDescription()
    {
        ModeDescription = ProxyMode switch
        {
            ProxyMode.All => "所有网站（局域网除外）经 8080 代理",
            ProxyMode.GfwOnly => "国外 IP + 白名单走代理，国内 IP 直连（GeoIP）",
            ProxyMode.Direct => "浏览器全部直连，隧道仍可手动用 SOCKS5",
            _ => "",
        };

        StatusHint = ProxyMode switch
        {
            ProxyMode.GfwOnly => "GeoIP 内置数据库 · 国外 IP 走代理",
            ProxyMode.All => "系统 HTTP 代理 127.0.0.1:8080",
            ProxyMode.Direct => "全部直连会关闭系统代理",
            _ => "",
        };
    }

    [RelayCommand(CanExecute = nameof(CanConnect))]
    private void Connect()
    {
        if (Status is ConnectionStatus.Connected or ConnectionStatus.Connecting)
            return;

        if (string.IsNullOrWhiteSpace(Remote))
        {
            AddLog("请先填写 MC 服务器地址（IP:端口）");
            return;
        }

        SaveConfig();
        Status = ConnectionStatus.Connecting;
        AddLog($"正在连接 {Remote}（{ProxyModeLabel(ProxyMode)}）...");

        if (!_tunnel.Start(BuildConfig(), _configService.ConfigPath))
        {
            Status = ConnectionStatus.Disconnected;
        }
    }

    private bool CanConnect() => Status != ConnectionStatus.Connecting && Status != ConnectionStatus.Connected;

    [RelayCommand(CanExecute = nameof(IsConnected))]
    private void Disconnect()
    {
        if (Status != ConnectionStatus.Connected)
            return;

        Status = ConnectionStatus.Disconnected;
        AddLog("正在断开…");
        _tunnel.Stop();
        AddLog("已断开连接");
        DisconnectCommand.NotifyCanExecuteChanged();
    }

    [RelayCommand]
    private void AddWhitelist()
    {
        var domain = ConfigService.NormalizeDomain(NewWhitelistDomain);
        if (string.IsNullOrEmpty(domain))
            return;

        if (Whitelist.Any(w => ConfigService.NormalizeDomain(w.Domain) == domain))
            return;

        Whitelist.Add(new WhitelistItemViewModel
        {
            Domain = domain,
            IncludeSubdomains = NewWhitelistSubdomains,
        });
        NewWhitelistDomain = "";
        SaveConfig();
    }

    [RelayCommand]
    private void RemoveWhitelist(WhitelistItemViewModel? item)
    {
        if (item is null) return;
        Whitelist.Remove(item);
        SaveConfig();
    }

    private void PollState()
    {
        if (Status == ConnectionStatus.Connected)
        {
            if (!_tunnel.IsRunning || !TunnelProcessService.IsProxyPortOpen())
            {
                Status = ConnectionStatus.Disconnected;
                AddLog("代理端口已关闭，连接已失效（请断开后重连）");
            }
            else
            {
                _ = PollTrafficAsync();
            }
            return;
        }

        if (Status != ConnectionStatus.Connecting)
            return;

        var tunnelUp = _logBuffer.Any(l => l.Contains("隧道已建立"));
        var portOpen = TunnelProcessService.IsProxyPortOpen();

        if (tunnelUp && portOpen)
        {
            Status = ConnectionStatus.Connected;
            AddLog("连接成功");
            return;
        }

        if (_logBuffer.Any(l =>
                l.Contains("[错误]") ||
                l.Contains("绑定 SOCKS5 端口失败") ||
                l.Contains("os error 10048") ||
                l.Contains("无法经雷神通道")))
        {
            Status = ConnectionStatus.Disconnected;
        }
    }

    private void OnLog(string line)
    {
        var app = System.Windows.Application.Current;
        if (app is null) return;
        app.Dispatcher.BeginInvoke(() => AddLog(line));
    }

    private void OnProcessExited()
    {
        var app = System.Windows.Application.Current;
        if (app is null) return;
        app.Dispatcher.BeginInvoke(() =>
        {
            if (Status != ConnectionStatus.Disconnected)
            {
                Status = ConnectionStatus.Disconnected;
                AddLog("隧道进程已退出");
            }
        });
    }

    private int _trafficPollRunning;

    private async Task PollTrafficAsync()
    {
        if (Interlocked.CompareExchange(ref _trafficPollRunning, 1, 0) != 0)
            return;

        try
        {
            if (Status != ConnectionStatus.Connected)
                return;

            var stats = await _trafficStats.FetchAsync().ConfigureAwait(false);
            if (stats is null || Status != ConnectionStatus.Connected)
                return;

            var app = System.Windows.Application.Current;
            app?.Dispatcher.BeginInvoke(() =>
            {
                if (Status != ConnectionStatus.Connected)
                    return;
                UploadSpeed = TrafficStatsService.FormatSpeed(stats.UpBps);
                DownloadSpeed = TrafficStatsService.FormatSpeed(stats.DownBps);
                UploadTotal = TrafficStatsService.FormatTotal(stats.UpTotal);
                DownloadTotal = TrafficStatsService.FormatTotal(stats.DownTotal);
            });
        }
        finally
        {
            Interlocked.Exchange(ref _trafficPollRunning, 0);
        }
    }

    private void ResetTrafficDisplay()
    {
        UploadSpeed = "0 B/s";
        DownloadSpeed = "0 B/s";
        UploadTotal = "0 B";
        DownloadTotal = "0 B";
    }

    private void AddLog(string line)
    {
        _logBuffer.Add(line);
        if (_logBuffer.Count > 150)
            _logBuffer.RemoveAt(0);

        Logs.Add(line);
        if (Logs.Count > 150)
            Logs.RemoveAt(0);
    }

    private static string ProxyModeLabel(ProxyMode mode) => mode switch
    {
        ProxyMode.All => "全部代理",
        ProxyMode.GfwOnly => "仅代理国外 IP",
        ProxyMode.Direct => "全部直连",
        _ => mode.ToString(),
    };

    public void Dispose()
    {
        _pollTimer.Stop();
        _tunnel.Dispose();
        _trafficStats.Dispose();
    }
}

public partial class WhitelistItemViewModel : ObservableObject
{
    [ObservableProperty] private string _domain = "";
    [ObservableProperty] private bool _includeSubdomains;
}
