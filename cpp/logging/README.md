## Helpful info for Event Viewer logging

This C++ project logs to the Windows Event Viewer. It's all wired up to be called from Rust just the same as our RPC code. If you want to test changes here:

1. Make sure to go change the `resourceFileName` and the `messageFileName` in
   `instrumentation.man` to point at where the files are in your build
   directory. (For me, that was
   `D:\dev\private\sudo\target\x86_64-pc-windows-msvc\debug\sudo.exe`). It needs
   to be the full path, so Event Viewer can find the exe (to load the resources
   from it to know how to format the packet of binary data written to it)
   - Make sure to change it back to `%systemroot%\System32\sudo.exe` before you push!
2. Make sure that Event Viewer is closed, and do
   ```bat
   wevtutil um cpp\logging\instrumentation.man
   ```
   to remove the old manifest from event viewer
3. Build the project
4. Do a
   ```bat
   wevtutil im cpp\logging\instrumentation.man
   ```
   to install the new manifest to event viewer
5. Open event viewer, and navigate to "Applications and Services Logs" ->
   "Microsoft" -> "Windows" -> "Sudo" -> "Admin"
   - alternatively:
     ```bat
     wevtutil qe Microsoft-Windows-Sudo/Admin /c:3 /rd:true /f:text
     ```
