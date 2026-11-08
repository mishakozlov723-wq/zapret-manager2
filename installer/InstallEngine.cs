using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Security.Cryptography;
using Microsoft.Win32;

namespace ManagerSetup;
/// <summary>Overall fraction 0..1 and an optional human note for the current stage.</summary>
public readonly record struct InstallProgress(double Fraction, string? Note);
internal static class InstallEngine
{
    internal const string RegistryPath = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\Zapret Manager";
    public static string? PreviousPath()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RegistryPath);
        return (key?.GetValue("InstallLocation") as string)?.Trim('"');
    }
    public static string DefaultPath => PreviousPath() ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Zapret Manager");
    public static string ValidatePath(string value)
    {
        var raw = value.Trim().Replace('/', '\\');
        if (raw.Length < 4 || raw.Length > 180 || !char.IsAsciiLetter(raw[0]) || raw[1] != ':' || raw[2] != '\\')
            throw new IOException("Выберите папку внутри локального диска, например D:\\Zapret Manager.");
        if (raw.Any(c => char.IsControl(c) || "\"<>|?*%".Contains(c)) || raw[2..].Contains(':')) throw new IOException("В пути есть недопустимые символы.");
        foreach (var part in raw[3..].Split('\\', StringSplitOptions.RemoveEmptyEntries))
        {
            var stem = part.Split('.')[0];
            if (part is "." or ".." || part.EndsWith('.') || part.EndsWith(' ') || System.Text.RegularExpressions.Regex.IsMatch(stem, "^(CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])$", System.Text.RegularExpressions.RegexOptions.IgnoreCase))
                throw new IOException("Недопустимое имя папки.");
        }
        var full = Path.GetFullPath(raw).TrimEnd('\\');
        if (full.Length < 4) throw new IOException("Нельзя устанавливать программу прямо в корень диска.");
        foreach (var protectedPath in new[] { Environment.GetFolderPath(Environment.SpecialFolder.Windows), Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86), Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData) })
            if (protectedPath.Length > 0 && IsWithin(full, protectedPath)) throw new IOException("Эта папка защищена Windows. Выберите личную папку или другой диск.");
        if (File.Exists(full)) throw new IOException("По этому пути находится файл, а не папка.");
        // Reject junctions so a harmless-looking destination cannot redirect into protected folders.
        for (var directory = new DirectoryInfo(full); directory != null; directory = directory.Parent)
            if (directory.Exists && (directory.Attributes & FileAttributes.ReparsePoint) != 0) throw new IOException("Выберите обычную папку, не ссылку на другой каталог.");
        return full;
    }
    static bool IsWithin(string path, string parent) => path.Equals(parent, StringComparison.OrdinalIgnoreCase) || path.StartsWith(parent.TrimEnd('\\') + "\\", StringComparison.OrdinalIgnoreCase);
    public static void CheckReady(string target)
    {
        var previous = PreviousPath();
        if (previous != null && !string.Equals(Path.GetFullPath(previous).TrimEnd('\\'), target, StringComparison.OrdinalIgnoreCase))
            throw new IOException("Менеджер уже установлен. Обновите его в прежней папке. Для переноса сначала удалите приложение через параметры Windows, сохранив данные.");
        foreach (var process in Process.GetProcessesByName("zapret-manager"))
        {
            using (process) { if (!process.HasExited) throw new IOException("Закройте только окно Zapret Manager и нажмите «Установить» снова. Службу и работающий обход отключать не нужно."); }
        }
        var drive = new DriveInfo(Path.GetPathRoot(target)!);
        if (!drive.IsReady || drive.DriveType != DriveType.Fixed) throw new IOException("Выберите доступный локальный диск.");
        if (drive.AvailableFreeSpace < 200L * 1024 * 1024) throw new IOException("На диске нужно не менее 200 МБ свободного места.");
        Directory.CreateDirectory(target);
        // Delete-on-close probe does not overwrite any existing user file.
        using var probe = new FileStream(Path.Combine(target, ".zm-write-" + Guid.NewGuid().ToString("N")), FileMode.CreateNew, FileAccess.Write, FileShare.None, 1, FileOptions.DeleteOnClose);
    }
    // Overall fraction where each stage ends; MainWindow shows the same stages.
    public const double UnpackEnd = 0.30, InstallEnd = 0.85, VerifyEnd = 0.97;

    static long ExpectedAppSize()
    {
        using var s = Assembly.GetExecutingAssembly().GetManifestResourceStream("Payload.app.size");
        if (s == null) return 0;
        using var r = new StreamReader(s);
        return long.TryParse(r.ReadToEnd().Trim(), out var v) ? v : 0;
    }

    public static async Task Install(string target, IProgress<InstallProgress> progress)
    {
        target = ValidatePath(target);
        CheckReady(target);
        var temporary = Directory.CreateTempSubdirectory("ZapretManager-setup-");
        try
        {
            // 1. Unpack: real byte progress of the embedded NSIS package.
            var payload = Path.Combine(temporary.FullName, "setup.exe");
            progress.Report(new(0, "Достаём установочный пакет…"));
            using (var source = Assembly.GetExecutingAssembly().GetManifestResourceStream("Payload.setup.exe") ?? throw new IOException("Встроенный пакет установки не найден."))
            using (var destination = new FileStream(payload, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                var total = Math.Max(1, source.Length); long done = 0; var buffer = new byte[256 * 1024]; int n;
                while ((n = await source.ReadAsync(buffer)) > 0)
                {
                    await destination.WriteAsync(buffer.AsMemory(0, n)); done += n;
                    progress.Report(new(UnpackEnd * done / total, null));
                }
            }

            // 2. Install: NSIS runs silently; progress = bytes of zapret-manager.exe written so far.
            progress.Report(new(UnpackEnd, "Записываем файлы программы…"));
            var exe = Path.Combine(target, "zapret-manager.exe");
            var expectedSize = ExpectedAppSize();
            var before = File.Exists(exe) ? (File.GetLastWriteTimeUtc(exe), new FileInfo(exe).Length) : ((DateTime, long)?)null;
            // NSIS requires /D to be the last argument, without quotes, even with spaces.
            // No shell is involved and ValidatePath rejects quotes and control characters.
            using var child = Process.Start(new ProcessStartInfo(payload) { Arguments = "/S /D=" + target, UseShellExecute = false, CreateNoWindow = true, WorkingDirectory = temporary.FullName }) ?? throw new IOException("Не удалось запустить установку.");
            var started = DateTime.UtcNow; var touched = before == null; var noted = false;
            while (!child.HasExited)
            {
                await Task.Delay(80);
                double fileFrac = 0;
                try
                {
                    if (File.Exists(exe))
                    {
                        var info = new FileInfo(exe);
                        // An update only counts once NSIS actually rewrites the old file.
                        if (!touched && before is { } b && (info.LastWriteTimeUtc != b.Item1 || info.Length != b.Item2)) touched = true;
                        if (touched && expectedSize > 0) fileFrac = Math.Min(1, (double)info.Length / expectedSize);
                    }
                }
                catch (IOException) { /* file is being written */ }
                var seconds = (DateTime.UtcNow - started).TotalSeconds;
                // Slow creep only so the bar never looks frozen (e.g. WebView2 check); never reaches the end before exit.
                var creep = 0.6 * (1 - Math.Exp(-seconds / 8));
                var frac = Math.Min(0.97, Math.Max(fileFrac, creep));
                string? note = null;
                if (fileFrac >= 1 && !noted) { note = "Файлы записаны, Windows завершает установку…"; noted = true; }
                progress.Report(new(UnpackEnd + (InstallEnd - UnpackEnd) * frac, note));
            }
            await child.WaitForExitAsync();
            if (child.ExitCode != 0) throw new IOException($"Установка не завершена (код {child.ExitCode}). Проверьте доступ к папке и подключение к интернету для WebView2, затем повторите.");
            if (!File.Exists(exe) || !File.Exists(Path.Combine(target, "uninstall.exe")))
                throw new IOException("Установщик завершился, но файлы приложения не найдены. Попробуйте ещё раз.");

            // 3. Verify: real byte progress of the SHA-256 over the installed exe.
            progress.Report(new(InstallEnd, "Сверяем контрольную сумму…"));
            using var expectedStream = Assembly.GetExecutingAssembly().GetManifestResourceStream("Payload.app.sha256") ?? throw new IOException("Нет контрольной суммы приложения.");
            using var reader = new StreamReader(expectedStream);
            var expected = (await reader.ReadToEndAsync()).Trim();
            string actual;
            using (var installed = File.OpenRead(exe))
            using (var sha = IncrementalHash.CreateHash(HashAlgorithmName.SHA256))
            {
                var total = Math.Max(1, installed.Length); long done = 0; var buffer = new byte[256 * 1024]; int n;
                while ((n = await installed.ReadAsync(buffer)) > 0)
                {
                    sha.AppendData(buffer, 0, n); done += n;
                    progress.Report(new(InstallEnd + (VerifyEnd - InstallEnd) * done / total, null));
                }
                actual = Convert.ToHexString(sha.GetHashAndReset());
            }
            if (!actual.Equals(expected, StringComparison.OrdinalIgnoreCase)) throw new IOException("Установленный файл не совпадает с пакетом. Повторите установку.");

            // 4. Register: confirm the uninstall entry Windows will show.
            progress.Report(new(VerifyEnd, "Проверяем запись в «Параметрах Windows»…"));
            var registered = PreviousPath();
            if (!string.Equals(registered?.TrimEnd('\\'), target, StringComparison.OrdinalIgnoreCase)) throw new IOException("Не удалось подтвердить регистрацию приложения в Windows.");
            progress.Report(new(1, null));
        }
        finally { try { temporary.Delete(true); } catch (IOException) { /* Windows may still hold the extracted package briefly. */ } }
    }
    public static void Launch(string folder) => Process.Start(new ProcessStartInfo(Path.Combine(folder, "zapret-manager.exe")) { UseShellExecute = true, WorkingDirectory = folder });
}
