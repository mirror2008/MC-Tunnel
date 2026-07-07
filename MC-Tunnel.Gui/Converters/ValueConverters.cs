using System.Globalization;
using System.Windows;
using System.Windows.Data;
using System.Windows.Media;
using MCTunnel.Gui.Models;

namespace MCTunnel.Gui.Converters;

public sealed class StatusToBrushConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is ConnectionStatus status)
        {
            return status switch
            {
                ConnectionStatus.Connected => new SolidColorBrush(Color.FromRgb(0x55, 0xFF, 0x55)),
                ConnectionStatus.Connecting => new SolidColorBrush(Color.FromRgb(0xFF, 0xAA, 0x00)),
                _ => new SolidColorBrush(Color.FromRgb(0x88, 0x88, 0x88)),
            };
        }
        return Brushes.Gray;
    }

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class StatusToTextConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is ConnectionStatus status)
        {
            return status switch
            {
                ConnectionStatus.Connected => "已连接",
                ConnectionStatus.Connecting => "连接中",
                _ => "未连接",
            };
        }
        return "未连接";
    }

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class InverseBoolConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is bool b ? !b : value ?? DependencyProperty.UnsetValue;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is bool b ? !b : value ?? DependencyProperty.UnsetValue;
}

public sealed class EnumToBoolConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value?.Equals(parameter) == true;

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is true ? parameter! : Binding.DoNothing;
}
