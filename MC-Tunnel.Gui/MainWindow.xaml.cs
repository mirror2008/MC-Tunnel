using System.ComponentModel;
using System.Windows;
using MCTunnel.Gui.ViewModels;

namespace MCTunnel.Gui;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (DataContext is MainViewModel vm)
        {
            vm.Logs.CollectionChanged += (_, _) =>
            {
                if (vm.Logs.Count > 0)
                {
                    Dispatcher.BeginInvoke(() => LogList?.ScrollIntoView(vm.Logs[^1]));
                }
            };
        }
    }

    protected override void OnClosing(CancelEventArgs e)
    {
        if (DataContext is MainViewModel vm)
        {
            vm.SaveAllSettings();
            vm.Dispose();
        }
        base.OnClosing(e);
    }
}
