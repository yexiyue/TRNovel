"""Development-only native process memory sampling (stdlib only)."""
import os, ctypes
from pathlib import Path

def memory(pid):
    if os.name == 'nt':
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [('cb', wintypes.DWORD), ('faults', wintypes.DWORD)] + [(name, ctypes.c_size_t) for name in ['peak_rss', 'rss', 'peak_pool', 'pool', 'peak_nonpaged', 'nonpaged', 'pagefile', 'peak_pagefile']]
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        psapi = ctypes.WinDLL('psapi', use_last_error=True)
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
        handle = kernel.OpenProcess(1040, False, pid)
        if not handle:
            return {}
        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        try:
            if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
                return dict(rss_bytes=counters.rss, peak_rss_bytes=counters.peak_rss)
        finally:
            kernel.CloseHandle(handle)
    elif Path(f'/proc/{pid}/status').exists():
        rows = dict((line.split(':', 1) for line in Path(f'/proc/{pid}/status').read_text().splitlines() if ':' in line))
        return {name: int(rows[key].split()[0]) * 1024 for name, key in [('rss_bytes', 'VmRSS'), ('peak_rss_bytes', 'VmHWM')] if key in rows}
    return {}
