using System.Globalization;
using System.Windows;
using System.Windows.Data;
using MCTunnel.Gui.Models;

namespace MCTunnel.Gui.Converters;

public sealed class TrafficVisibilityConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is ConnectionStatus.Connected ? Visibility.Visible : Visibility.Collapsed;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}
