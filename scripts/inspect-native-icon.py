"""Read executable and window icons; do not modify services or settings."""
import ctypes as C
from ctypes import wintypes as W
from pathlib import Path
from PIL import Image
u=C.WinDLL('user32'); g=C.WinDLL('gdi32'); s=C.WinDLL('shell32'); k=C.WinDLL('kernel32')
g.CreateCompatibleDC.argtypes=[W.HDC]; g.CreateCompatibleDC.restype=W.HDC
g.CreateDIBSection.restype=W.HBITMAP
g.CreateDIBSection.argtypes=[W.HDC,C.c_void_p,W.UINT,C.POINTER(C.c_void_p),W.HANDLE,W.DWORD]
g.SelectObject.argtypes=[W.HDC,W.HANDLE]; g.SelectObject.restype=W.HANDLE
u.DrawIconEx.argtypes=[W.HDC,C.c_int,C.c_int,W.HICON,C.c_int,C.c_int,W.UINT,W.HBRUSH,W.UINT]
s.ExtractIconExW.argtypes=[W.LPCWSTR,C.c_int,C.POINTER(W.HICON),C.POINTER(W.HICON),W.UINT]
u.SendMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM]; u.SendMessageW.restype=C.c_void_p
k.OpenProcess.argtypes=[W.DWORD,W.BOOL,W.DWORD]; k.OpenProcess.restype=W.HANDLE
k.QueryFullProcessImageNameW.argtypes=[W.HANDLE,W.DWORD,W.LPWSTR,C.POINTER(W.DWORD)]
k.CloseHandle.argtypes=[W.HANDLE]
class Header(C.Structure):
    _fields_=[('size',W.DWORD),('width',W.LONG),('height',W.LONG),('planes',W.WORD),('bits',W.WORD),('compression',W.DWORD),('sizeImage',W.DWORD),('x',W.LONG),('y',W.LONG),('used',W.DWORD),('important',W.DWORD)]
out=Path('icon-verification'); out.mkdir(exist_ok=True)
def save(icon,name):
    if not icon: print(name,'NO ICON'); return
    dc=g.CreateCompatibleDC(None); header=Header(40,64,-64,1,32,0,0,0,0,0,0); bits=C.c_void_p()
    bm=g.CreateDIBSection(dc,C.byref(header),0,C.byref(bits),None,0); g.SelectObject(dc,bm)
    C.memset(bits,35,64*64*4); u.DrawIconEx(dc,0,0,icon,64,64,0,None,3)
    Image.frombytes('RGB',(64,64),C.string_at(bits,64*64*4),'raw','BGRX').save(out/(name+'.png'))
    print('SAVED',name)
exe=Path('src-tauri/target/release/zapret-manager.exe').resolve()
large,small=W.HICON(),W.HICON()
s.ExtractIconExW(str(exe),0,C.byref(large),C.byref(small),1)
save(large,'exe-large'); save(small,'exe-small')
CALLBACK=C.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
@CALLBACK
def visit(hwnd,_):
    title=C.create_unicode_buffer(512); u.GetWindowTextW(hwnd,title,512)
    if title.value!='Zapret Manager': return True
    pid=W.DWORD(); u.GetWindowThreadProcessId(hwnd,C.byref(pid))
    handle=k.OpenProcess(0x1000,False,pid.value); path=C.create_unicode_buffer(32768); size=W.DWORD(32768)
    k.QueryFullProcessImageNameW(handle,0,path,C.byref(size)); k.CloseHandle(handle)
    print('WINDOW',pid.value,path.value)
    from PIL import ImageGrab
    rect=W.RECT(); u.GetWindowRect(hwnd,C.byref(rect))
    ImageGrab.grab(bbox=(rect.left,rect.top,rect.right,rect.bottom)).save(out/('window-'+str(pid.value)+'.png'))
    save(u.SendMessageW(hwnd,0x7f,0,0),'window-small-'+str(pid.value))
    save(u.SendMessageW(hwnd,0x7f,1,0),'window-large-'+str(pid.value))
    return True
u.EnumWindows(visit,0)
