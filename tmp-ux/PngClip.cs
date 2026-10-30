// Put a PNG-only entry on the clipboard (registered format "PNG", no DIB companion),
// with a marker text also available? NO — PNG-ONLY is the point: exercises the fixed
// capture branch, not the DIB fallback.
public class PngClip {
  [System.Runtime.InteropServices.DllImport("user32.dll")]
  static extern bool OpenClipboard(System.IntPtr h);
  [System.Runtime.InteropServices.DllImport("user32.dll")]
  static extern bool CloseClipboard();
  [System.Runtime.InteropServices.DllImport("user32.dll")]
  static extern bool EmptyClipboard();
  [System.Runtime.InteropServices.DllImport("user32.dll")]
  static extern System.IntPtr SetClipboardData(uint fmt, System.IntPtr h);
  [System.Runtime.InteropServices.DllImport("kernel32.dll")]
  static extern System.IntPtr GlobalAlloc(uint flags, System.UIntPtr bytes);
  [System.Runtime.InteropServices.DllImport("kernel32.dll")]
  static extern System.IntPtr GlobalLock(System.IntPtr h);
  [System.Runtime.InteropServices.DllImport("kernel32.dll")]
  static extern bool GlobalUnlock(System.IntPtr h);
  [System.Runtime.InteropServices.DllImport("user32.dll")]
  static extern uint RegisterClipboardFormatW([System.Runtime.InteropServices.MarshalAs(System.Runtime.InteropServices.UnmanagedType.LPWStr)] string name);

  public static uint Put(string pngPath) {
    uint CF_UNICODETEXT = 13;
    byte[] png = System.IO.File.ReadAllBytes(pngPath);
    if (!OpenClipboard(System.IntPtr.Zero)) return 0;
    try {
      EmptyClipboard();
      uint pngFmt = RegisterClipboardFormatW("PNG");
      System.IntPtr h = GlobalAlloc(0x2002 /*GMEM_MOVEABLE|GMEM_ZEROINIT*/, (System.UIntPtr)png.Length);
      System.IntPtr p = GlobalLock(h);
      System.Runtime.InteropServices.Marshal.Copy(png, 0, p, png.Length);
      GlobalUnlock(h);
      System.IntPtr r = SetClipboardData(pngFmt, h);
      System.IntPtr t = SetClipboardData(CF_UNICODETEXT, System.IntPtr.Zero); // no text: png-only
      return r == System.IntPtr.Zero ? 0 : pngFmt;
    } finally { CloseClipboard(); }
  }
}
