Option Explicit
Dim shell, fso, command, argument, powershell, scriptPath
Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
powershell = fso.BuildPath(fso.GetSpecialFolder(1), "WindowsPowerShell\v1.0\powershell.exe")
If WScript.Arguments.Count > 0 Then
    If WScript.Arguments(0) = "-Dispatch" Then
        If WScript.Arguments.Count <> 2 Then WScript.Quit 2
        command = Quote(fso.BuildPath(fso.GetAbsolutePathName(WScript.Arguments(1)), "opendiy-codex-launcher.exe")) & " update"
        WScript.Quit shell.Run(command, 0, True)
    End If
End If
scriptPath = fso.BuildPath(fso.GetParentFolderName(WScript.ScriptFullName), "update.ps1")
command = Quote(powershell) & " -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File " & Quote(scriptPath)
For Each argument In WScript.Arguments
    command = command & " " & Quote(argument)
Next
WScript.Quit shell.Run(command, 0, True)

Function Quote(value)
    If InStr(value, Chr(34)) > 0 Or InStr(value, vbCr) > 0 Or InStr(value, vbLf) > 0 Then WScript.Quit 2
    Quote = Chr(34) & value & Chr(34)
End Function
