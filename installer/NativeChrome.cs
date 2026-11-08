using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace ManagerSetup;
internal static class NativeChrome
{
    [DllImport("dwmapi.dll")] private static extern int DwmSetWindowAttribute(IntPtr window, int attribute, ref int value, int size);
    public static void Apply(Window window)
    {
        window.SourceInitialized += (_, _) =>
        {
            var handle = new WindowInteropHelper(window).Handle;
            var enabled = 1;
            // Unsupported attributes on older Windows are simply ignored by DWM.
            DwmSetWindowAttribute(handle, 20, ref enabled, sizeof(int));
            var caption = 0x00171717;
            DwmSetWindowAttribute(handle, 35, ref caption, sizeof(int));
            var text = 0x00EDEDED;
            DwmSetWindowAttribute(handle, 36, ref text, sizeof(int));
        };
    }
}
