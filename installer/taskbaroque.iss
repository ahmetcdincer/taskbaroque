#define AppName "Taskbaroque"
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppExe "taskbaroque.exe"

[Setup]
AppId={{6B1E3C52-8F4A-4D7B-9C2E-5A1F0D3B7E91}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist
OutputBaseFilename=Taskbaroque-Setup-{#AppVersion}
SetupIconFile=..\assets\taskbaroque.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=no

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
turkish.AutoStart=Windows ile başlat
english.AutoStart=Start with Windows
turkish.Launch=Taskbaroque'u şimdi başlat
english.Launch=Launch Taskbaroque now

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "{cm:AutoStart}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Taskbaroque"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:Launch}"; Flags: nowait postinstall skipifsilent

[Code]
function CloseRunning(): Boolean;
var
  h, i, rc: Integer;
begin
  Result := False;
  h := FindWindowByWindowName('TaskbaroqueHost');
  if h = 0 then
    Exit;
  PostMessage(h, 16, 0, 0);
  i := 0;
  while (FindWindowByWindowName('TaskbaroqueHost') <> 0) and (i < 50) do
  begin
    Sleep(100);
    i := i + 1;
  end;
  if FindWindowByWindowName('TaskbaroqueHost') <> 0 then
  begin
    Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#AppExe}', '', SW_HIDE, ewWaitUntilTerminated, rc);
    Result := True;
  end;
  Sleep(300);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseRunning();
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  rc: Integer;
begin
  if CurStep = ssPostInstall then
    Exec(ExpandConstant('{app}\{#AppExe}'), '--integrate', '', SW_HIDE, ewWaitUntilTerminated, rc);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  rc: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    if CloseRunning() then
      Exec(ExpandConstant('{app}\{#AppExe}'), '--restore', '', SW_HIDE, ewWaitUntilTerminated, rc);
    Exec(ExpandConstant('{app}\{#AppExe}'), '--unregister', '', SW_HIDE, ewWaitUntilTerminated, rc);
  end;
end;
