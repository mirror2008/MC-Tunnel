using System.Globalization;
using System.Windows.Data;
using MCTunnel.Gui.Models;

namespace MCTunnel.Gui.Converters;

public sealed class SpeedModeDisplayConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is SpeedMode s ? s switch
        {
            SpeedMode.Balanced => "均衡 Balanced",
            SpeedMode.Stealth => "隐蔽 Stealth",
            _ => "极速 Fast",
        } : "";

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value?.ToString() switch
        {
            "均衡 Balanced" => SpeedMode.Balanced,
            "隐蔽 Stealth" => SpeedMode.Stealth,
            _ => SpeedMode.Fast,
        };
}
