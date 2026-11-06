; Unsigned Windows 11 x64 development installer. Build through the PowerShell wrapper.
#ifndef PayloadDirectory
  #error PayloadDirectory is required
#endif
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef SourceRevision
  #error SourceRevision is required
#endif
#if Len(PayloadDirectory) == 0
  #error Empty payload directory is forbidden
#endif
#if !FileExists(PayloadDirectory + "\INSTALLER-IDENTITY.ini")
  #error The payload must include its generated installer identity
#endif
#define KlickyAppId "{B8E55779-4EDB-4D8F-962D-10460EC7C230}"

[Setup]
AppId={{B8E55779-4EDB-4D8F-962D-10460EC7C230}
AppName=Klicky
AppVersion={#AppVersion}
AppVerName=Klicky {#AppVersion} Windows development preview
AppPublisher=Muneer Alam and Sauhard Gupta
AppPublisherURL=https://github.com/Muneer320/klicky
AppSupportURL=https://github.com/Muneer320/klicky/issues
AppUpdatesURL=https://github.com/Muneer320/klicky/releases
AppComments=Unsigned development preview; physical and installation qualification remain open.
DefaultDirName={localappdata}\Programs\Klicky
DisableDirPage=yes
DisableProgramGroupPage=yes
UsePreviousAppDir=no
UsePreviousTasks=no
PrivilegesRequired=lowest
SetupArchitecture=x64
ArchitecturesAllowed=x64os
MinVersion=10.0.22000
SetupIconFile=klicky.ico
UninstallDisplayIcon={app}\klicky-desktop.exe
UninstallDisplayName=Klicky Windows development preview
UninstallLogMode=append
CloseApplications=no
RestartApplications=no
RestartIfNeededByRun=no
AlwaysRestart=no
AllowCancelDuringInstall=no
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
OutputBaseFilename=klicky-{#AppVersion}-windows-x64-preview-{#Copy(SourceRevision, 1, 12)}-setup
VersionInfoVersion={#AppVersion}.0
VersionInfoProductTextVersion={#AppVersion} Windows development preview
VersionInfoDescription=Klicky unsigned development installer
VersionInfoOriginalFileName=klicky-setup.exe
SignedUninstaller=no

[Languages]
Name: english; MessagesFile: compiler:Default.isl

[Tasks]
Name: startup; Description: "Start Klicky when I sign in (Windows may independently disable this)"; Flags: unchecked; Check: FreshInstall

[Files]
Source: "{#PayloadDirectory}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Klicky"; Filename: "{app}\klicky-desktop.exe"; Parameters: "--paused"; WorkingDir: "{app}"; Check: CreateStartShortcut

[Run]
Filename: "{app}\klicky-desktop.exe"; Parameters: "--paused"; Description: "Open Klicky paused"; Flags: postinstall nowait skipifsilent unchecked; Check: OfferLaunch

[Code]
const
  MarkerName = '.klicky.bundle.lock';
  IdentityName = 'INSTALLER-IDENTITY.ini';
  GenericReadWrite = $C0000000;
  OpenAlways = 4;
  OpenReparsePoint = $00200000;
  ReparsePoint = $400;
  DirectoryAttribute = $10;
  InvalidAttributes = $FFFFFFFF;
  UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#KlickyAppId}_is1';
type
  TNativeFindData = record
    Attributes: LongWord;
    CreationTime, LastAccessTime, LastWriteTime: TFileTime;
    SizeHigh, SizeLow, Reserved0, Reserved1: LongWord;
    Name: array[0..259] of Word;
    AlternateName: array[0..13] of Word;
  end;
var
  BundleHandle: THandle;
  Prepared, Committed, WasRunning, WasPaused, HadInstall, RemovalStarted, PostInstallWarning: Boolean;
  BackupDirectory: String;
  BackupFiles, BackupHashes: TStringList;
  BackupBytes: Int64;
  SnapshotEntries: Integer;
  HadUninstallKey: Boolean;
  UninstallKeyHash: String;
  HadStartShortcut: Boolean;
  StartShortcutHash: String;
  OfflineRecovery: Boolean;

function CreateFileW(Name: String; Access, Share: LongWord; Security: NativeUInt;
  Disposition, Flags: LongWord; Template: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';
function GetFileAttributesW(Name: String): LongWord;
  external 'GetFileAttributesW@kernel32.dll stdcall';
function GetFileSize(Handle: THandle; var High: LongWord): LongWord;
  external 'GetFileSize@kernel32.dll stdcall';
function CreateDirectoryW(Name: String; Security: NativeUInt): Boolean;
  external 'CreateDirectoryW@kernel32.dll stdcall';
function NativeFindFirst(Name: String; var Data: TNativeFindData): THandle;
  external 'FindFirstFileW@kernel32.dll stdcall';
function NativeFindNext(Handle: THandle; var Data: TNativeFindData): Boolean;
  external 'FindNextFileW@kernel32.dll stdcall';

function AttributesIfPresent(const Path: String): LongWord;
var
  Error: Integer;
begin
  Result := GetFileAttributesW(Path);
  if Result = InvalidAttributes then begin
    Error := DLLGetLastError;
    if (Error <> 2) and (Error <> 3) then
      RaiseException('Cannot inspect the managed path: ' + SysErrorMessage(Error));
  end;
end;

procedure CopyFindData(const Data: TNativeFindData; var Found: TFindRec);
var
  I: Integer;
begin
  Found.Name := '';
  for I := 0 to 259 do begin
    if Data.Name[I] = 0 then Break;
    Found.Name := Found.Name + Chr(Data.Name[I]);
  end;
  Found.Attributes := Data.Attributes;
  Found.SizeHigh := Data.SizeHigh;
  Found.SizeLow := Data.SizeLow;
end;

function FindFirstChecked(const Pattern: String; var Found: TFindRec): Boolean;
var
  Data: TNativeFindData;
  Error: Integer;
begin
  Found.FindHandle := NativeFindFirst(Pattern, Data);
  if Found.FindHandle = THandle(-1) then begin
    Error := DLLGetLastError;
    Found.FindHandle := 0;
    if (Error <> 2) and (Error <> 18) then
      RaiseException('Cannot enumerate the managed folder: ' + SysErrorMessage(Error));
    Result := False;
  end else begin
    CopyFindData(Data, Found);
    Result := True;
  end;
end;

function FindNextChecked(var Found: TFindRec): Boolean;
var
  Data: TNativeFindData;
  Error: Integer;
begin
  Result := NativeFindNext(Found.FindHandle, Data);
  if Result then CopyFindData(Data, Found)
  else begin
    Error := DLLGetLastError;
    if Error <> 18 then RaiseException('The managed folder scan failed: ' + SysErrorMessage(Error));
  end;
end;

function CreateStartShortcut: Boolean;
begin
  { Existing entries need no rewrite: the target path is stable across upgrades.
    Skipping them also keeps Inno rollback from deleting a prior shortcut. }
  Result := AttributesIfPresent(ExpandConstant('{userprograms}\Klicky.lnk')) = InvalidAttributes;
end;

function FreshInstall: Boolean;
begin
  Result := not FileExists(ExpandConstant('{localappdata}\Programs\Klicky\') + IdentityName);
end;

procedure CheckPath(const Path: String; Directory: Boolean);
var
  Attributes: LongWord;
begin
  Attributes := AttributesIfPresent(Path);
  if (Attributes = InvalidAttributes) or ((Attributes and ReparsePoint) <> 0) or
     (((Attributes and DirectoryAttribute) <> 0) <> Directory) then
    RaiseException('Klicky requires an ordinary local path: ' + Path);
end;

procedure CheckParents(Path: String);
begin
  while Length(Path) > 3 do begin
    if AttributesIfPresent(Path) <> InvalidAttributes then CheckPath(Path, True);
    Path := ExtractFileDir(Path);
  end;
end;

procedure CheckInstallLocation;
var
  Path: String;
begin
  Path := ExpandConstant('{app}');
  if (Length(Path) < 3) or (Path[2] <> ':') or
     (CompareText(RemoveBackslashUnlessRoot(Path), ExpandConstant('{localappdata}\Programs\Klicky')) <> 0) then
    RaiseException('This preview installs only in the current user''s local Programs\Klicky folder. Custom or network install locations are not supported.');
  CheckParents(Path);
end;

procedure UnlockBundle;
begin
  if BundleHandle <> 0 then begin
    CloseHandle(BundleHandle);
    BundleHandle := 0;
  end;
end;

procedure LockBundle;
var
  Marker: String;
  High, Low: LongWord;
begin
  Marker := AddBackslash(ExpandConstant('{app}')) + MarkerName;
  BundleHandle := CreateFileW(Marker, GenericReadWrite, 0, 0, OpenAlways, OpenReparsePoint, 0);
  if BundleHandle = THandle(-1) then begin
    BundleHandle := 0;
    RaiseException('Another Klicky process or installer is using this bundle. Close Klicky in every signed-in session and retry. No process will be forced to close.');
  end;
  try
    CheckPath(Marker, False);
    High := 0;
    Low := GetFileSize(BundleHandle, High);
    if (Low <> 0) or (High <> 0) then RaiseException('The Klicky bundle ownership marker must be empty.');
  except
    UnlockBundle;
    RaiseException(GetExceptionMessage);
  end;
end;

function RunControl(const Arguments: String; RequireReceipt: Boolean): String;
var
  Code, I: Integer;
  Output: TExecOutput;
begin
  if not ExecAndCaptureOutput(ExpandConstant('{app}\klicky.exe'), Arguments,
       ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, Code, Output) then
    RaiseException('Could not run Klicky preparation: ' + SysErrorMessage(Code));
  if (Code <> 0) or Output.Error or (RequireReceipt and (GetArrayLength(Output.StdOut) <> 1)) then begin
    Log('Klicky hook exit=' + IntToStr(Code) + ', stdout lines=' + IntToStr(GetArrayLength(Output.StdOut)));
    for I := 0 to GetArrayLength(Output.StdOut) - 1 do
      if I < 16 then Log(Output.StdOut[I]);
    for I := 0 to GetArrayLength(Output.StdErr) - 1 do
      if I < 16 then Log(Output.StdErr[I]);
    RaiseException('Klicky could not confirm safe preparation. Close this bundle and retry. See the installer log for details.');
  end;
  Result := '';
  if RequireReceipt then Result := Trim(Output.StdOut[0]);
end;

function CheckIdentity: Boolean;
var
  Identity, Expected, OldVersion: String;
  OldPacked, NewPacked: Int64;
begin
  Identity := ExpandConstant('{app}\') + IdentityName;
  CheckPath(Identity, False);
  if GetIniString('Klicky', 'AppId', '', Identity) <> '{#KlickyAppId}' then
    RaiseException('This folder is not owned by the Klicky installer. Move portable copies elsewhere first.');
  Expected := GetIniString('Klicky', 'CliSHA256', '', Identity);
  if Length(Expected) <> 64 then RaiseException('The installed control identity is invalid.');
  Result := AttributesIfPresent(ExpandConstant('{app}\klicky.exe')) <> InvalidAttributes;
  if Result then begin
    CheckPath(ExpandConstant('{app}\klicky.exe'), False);
    if CompareText(GetSHA256OfFile(ExpandConstant('{app}\klicky.exe')), Expected) <> 0 then
      RaiseException('The installed control executable differs from its installer identity. It was not executed.');
  end;
  if IsUninstaller then Exit;
  OldVersion := GetIniString('Klicky', 'Version', '', Identity);
  if not StrToVersion(OldVersion, OldPacked) or not StrToVersion('{#AppVersion}', NewPacked) then
    RaiseException('The installer version identity is invalid.');
  if ComparePackedVersion(OldPacked, NewPacked) > 0 then
    RaiseException('Downgrading Klicky is not supported. Remove the newer application first; your configuration and custom packs will remain.');
end;

procedure PrepareProcess;
var
  Receipt: String;
begin
  Receipt := RunControl('service prepare-update', True);
  if (Receipt <> 'KLICKY_UPDATE_V1 0 0') and (Receipt <> 'KLICKY_UPDATE_V1 0 1') and
     (Receipt <> 'KLICKY_UPDATE_V1 1 0') and (Receipt <> 'KLICKY_UPDATE_V1 1 1') then
    RaiseException('The installed Klicky version did not return a supported preparation receipt.');
  WasRunning := Receipt[18] = '1';
  WasPaused := Receipt[20] = '1';
end;

#include "recovery.iss"

procedure SnapshotDirectory(const Relative: String; Depth: Integer);
var
  Found: TFindRec;
  Name, Source, Destination, Hash: String;
begin
  if Depth > 12 then RaiseException('Too many nested folders in the installed bundle.');
  Source := AddBackslash(ExpandConstant('{app}')) + Relative;
  CheckPath(RemoveBackslashUnlessRoot(Source), True);
  if FindFirstChecked(AddBackslash(Source) + '*', Found) then try
    repeat
      if (Found.Name <> '.') and (Found.Name <> '..') then begin
        SnapshotEntries := SnapshotEntries + 1;
        if SnapshotEntries > 8192 then RaiseException('The managed bundle exceeds the directory-entry backup limit.');
        Name := AddBackslash(Relative) + Found.Name;
        if Relative = '' then Name := Found.Name;
        if (Found.Attributes and ReparsePoint) <> 0 then
          RaiseException('Reparse points are not permitted in the managed bundle: ' + Name);
        if (Found.Attributes and DirectoryAttribute) <> 0 then
          SnapshotDirectory(Name, Depth + 1)
        else if CompareText(Name, MarkerName) <> 0 then begin
          if (Found.SizeHigh <> 0) or (Found.SizeLow > 268435456) or (BackupFiles.Count >= 4096) then
            RaiseException('The managed bundle exceeds installer backup limits.');
          BackupBytes := BackupBytes + Found.SizeLow;
          if BackupBytes > 536870912 then RaiseException('The managed bundle exceeds the 512 MiB backup limit.');
          Source := AddBackslash(ExpandConstant('{app}')) + Name;
          Destination := AddBackslash(BackupDirectory) + 'payload\' + Name;
          if not ForceDirectories(ExtractFileDir(Destination)) then RaiseException('Cannot create recovery folder.');
          Hash := GetSHA256OfFile(Source);
          if not CopyFile(Source, Destination, True) then RaiseException('Cannot back up ' + Name);
          if CompareText(GetSHA256OfFile(Destination), Hash) <> 0 then RaiseException('Recovery copy failed verification: ' + Name);
          BackupFiles.Add(Name);
          BackupHashes.Add(Hash);
        end;
      end;
    until not FindNextChecked(Found);
  finally
    FindClose(Found);
  end;
end;

function RunRegistry(const Arguments: String): Boolean;
var
  Code: Integer;
begin
  Result := Exec(ExpandConstant('{sys}\reg.exe'), Arguments, '', SW_HIDE,
    ewWaitUntilTerminated, Code) and (Code = 0);
end;

procedure BackupUninstallKey;
var
  Path: String;
begin
  HadUninstallKey := RegKeyExists(HKCU64, UninstallKey);
  if not HadUninstallKey then Exit;
  Path := AddBackslash(BackupDirectory) + 'uninstall.reg';
  if not RunRegistry('export "HKCU\' + UninstallKey + '" "' + Path + '" /y /reg:64') then
    RaiseException('Cannot back up the previous Installed Apps registration.');
  UninstallKeyHash := GetSHA256OfFile(Path);
end;

function RestoreUninstallKey: Boolean;
var
  Path, Verified: String;
begin
  Result := True;
  if not HadUninstallKey then begin
    if RegKeyExists(HKCU64, UninstallKey) then Result := RegDeleteKeyIncludingSubkeys(HKCU64, UninstallKey);
    Exit;
  end;
  Path := AddBackslash(BackupDirectory) + 'uninstall.reg';
  Verified := AddBackslash(BackupDirectory) + 'uninstall-restored.reg';
  CheckPath(Path, False);
  if CompareText(GetSHA256OfFile(Path), UninstallKeyHash) <> 0 then begin
    Result := False;
    Exit;
  end;
  if RegKeyExists(HKCU64, UninstallKey) and not RegDeleteKeyIncludingSubkeys(HKCU64, UninstallKey) then begin
    Result := False;
    Exit;
  end;
  Result := RunRegistry('import "' + Path + '" /reg:64') and
    RunRegistry('export "HKCU\' + UninstallKey + '" "' + Verified + '" /y /reg:64');
  if Result then Result := CompareText(GetSHA256OfFile(Verified), UninstallKeyHash) = 0;
end;

procedure BackupStartShortcut;
var
  Path, Destination: String;
begin
  Path := ExpandConstant('{userprograms}\Klicky.lnk');
  CheckParents(ExtractFileDir(Path));
  HadStartShortcut := AttributesIfPresent(Path) <> InvalidAttributes;
  if not HadStartShortcut then Exit;
  CheckPath(Path, False);
  Destination := AddBackslash(BackupDirectory) + 'start-menu.lnk';
  StartShortcutHash := GetSHA256OfFile(Path);
  if not CopyFile(Path, Destination, True) or
     (CompareText(GetSHA256OfFile(Destination), StartShortcutHash) <> 0) then
    RaiseException('Cannot back up the previous Start menu entry.');
end;

function RestoreStartShortcut: Boolean;
var
  Path, Source: String;
begin
  Path := ExpandConstant('{userprograms}\Klicky.lnk');
  CheckParents(ExtractFileDir(Path));
  Result := AttributesIfPresent(Path) = InvalidAttributes;
  if not HadStartShortcut then Exit;
  if not Result then begin
    CheckPath(Path, False);
    Result := CompareText(GetSHA256OfFile(Path), StartShortcutHash) = 0;
    if Result then Exit;
  end;
  Source := AddBackslash(BackupDirectory) + 'start-menu.lnk';
  CheckPath(Source, False);
  Result := (CompareText(GetSHA256OfFile(Source), StartShortcutHash) = 0) and
    ForceDirectories(ExtractFileDir(Path)) and CopyFile(Source, Path, False);
  if Result then Result := CompareText(GetSHA256OfFile(Path), StartShortcutHash) = 0;
end;

procedure MakeBackup;
var
  Parent: String;
begin
  Parent := ExpandConstant('{localappdata}\KlickyInstaller\recovery');
  CheckParents(Parent);
  if not ForceDirectories(Parent) then RaiseException('Cannot create the recovery parent folder.');
  BackupDirectory := GenerateUniqueName(Parent, '.backup');
  if not CreateDirectoryW(BackupDirectory, 0) then begin
    BackupDirectory := '';
    RaiseException('Cannot create a new persistent recovery copy.');
  end;
  BackupFiles := TStringList.Create;
  BackupHashes := TStringList.Create;
  SnapshotDirectory('', 0);
  BackupFiles.SaveToFile(AddBackslash(BackupDirectory) + 'RECOVERY-FILES.txt');
  BackupHashes.SaveToFile(AddBackslash(BackupDirectory) + 'RECOVERY-SHA256.txt');
  BackupUninstallKey;
  BackupStartShortcut;
  Log('Verified previous bundle recovery copy: ' + BackupDirectory);
end;

function RestoreBackup: Boolean;
var
  I: Integer;
  Source, Destination: String;
begin
  Result := True;
  for I := 0 to BackupFiles.Count - 1 do begin
    Source := AddBackslash(BackupDirectory) + 'payload\' + BackupFiles[I];
    Destination := ExpandConstant('{app}\') + BackupFiles[I];
    try
      CheckParents(ExtractFileDir(Source));
      CheckPath(Source, False);
      CheckParents(ExtractFileDir(Destination));
      if FileExists(Destination) then CheckPath(Destination, False);
      if CompareText(GetSHA256OfFile(Source), BackupHashes[I]) <> 0 then
        Result := False
      else if FileExists(Destination) and
         (CompareText(GetSHA256OfFile(Destination), BackupHashes[I]) = 0) then
        Log('Previous file already verified; preserving ' + BackupFiles[I])
      else if not ForceDirectories(ExtractFileDir(Destination)) or
         not CopyFile(Source, Destination, False) or
         (CompareText(GetSHA256OfFile(Destination), BackupHashes[I]) <> 0) then Result := False;
    except
      Log(GetExceptionMessage);
      Result := False;
    end;
  end;
  try
    if not RestoreUninstallKey then Result := False;
    if not RestoreStartShortcut then Result := False;
  except
    Log(GetExceptionMessage);
    Result := False;
  end;
end;

procedure RestartPrevious;
var
  Arguments: String;
begin
  if not WasRunning then Exit;
  WasRunning := False;
  Arguments := 'service start';
  if WasPaused then Arguments := Arguments + ' --paused';
  try
    CheckIdentity;
    RunControl(Arguments, False);
  except
    PostInstallWarning := True;
    Log(GetExceptionMessage);
    SuppressibleMsgBox('Klicky files are ready, but restarting was not confirmed. Open Klicky from Start when ready.', mbError, MB_OK, IDOK);
  end;
end;

procedure CheckEmptyDestination;
var
  Found: TFindRec;
begin
  if not DirExists(ExpandConstant('{app}')) then Exit;
  if FindFirstChecked(ExpandConstant('{app}\*'), Found) then try
    repeat
      if (Found.Name <> '.') and (Found.Name <> '..') and (Found.Name <> MarkerName) then
        RaiseException('The destination is not an empty installer-owned Klicky folder. Portable copies are preserved.');
    until not FindNextChecked(Found);
  finally
    FindClose(Found);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if Prepared then Exit;
  try
    if BackupDirectory <> '' then RaiseException('A preparation backup already exists. Cancel this installer before retrying; the recovery copy is preserved at ' + BackupDirectory);
    CheckInstallLocation;
    HadInstall := FileExists(ExpandConstant('{app}\') + IdentityName);
    if HadInstall then begin
      OfflineRecovery := not CheckIdentity;
      if not OfflineRecovery then PrepareProcess;
    end else CheckEmptyDestination;
    if not ForceDirectories(ExpandConstant('{app}')) then RaiseException('Cannot create the Klicky application folder.');
    LockBundle;
    if HadInstall <> FileExists(ExpandConstant('{app}\') + IdentityName) then
      RaiseException('Klicky installation ownership changed during preparation. Cancel and retry.');
    if HadInstall then begin
      if OfflineRecovery = CheckIdentity then
        RaiseException('The installed control executable changed during preparation. Cancel and retry.');
      if OfflineRecovery then Log('Repairing an inactive owned bundle without running its missing control executable.');
      MakeBackup;
    end else CheckEmptyDestination;
    Prepared := True;
  except
    Result := GetExceptionMessage;
    UnlockBundle;
    RestartPrevious;
  end;
end;

function OfferLaunch: Boolean;
begin
  Result := not HadInstall;
end;

procedure CurStepChanged(Step: TSetupStep);
begin
  if Step = ssPostInstall then begin
    Committed := True;
    UnlockBundle;
    if not HadInstall and WizardIsTaskSelected('startup') then try
      if RunControl('service register-startup', True) <> 'KLICKY_STARTUP_V1 REGISTERED' then
        RaiseException('Unexpected login registration response.');
    except
      PostInstallWarning := True;
      Log(GetExceptionMessage);
      SuppressibleMsgBox('Klicky was installed, but login startup could not be registered. Use its tray menu to inspect the conflict. Existing startup entries were preserved.', mbError, MB_OK, IDOK);
    end;
    RestartPrevious;
  end;
end;

procedure DeinitializeSetup;
var
  Restored: Boolean;
begin
  Restored := True;
  if Prepared and not Committed and HadInstall then Restored := RestoreBackup;
  UnlockBundle;
  if Prepared and not Committed and not HadInstall then begin
    DeleteFile(ExpandConstant('{app}\') + MarkerName);
    RemoveDir(ExpandConstant('{app}'));
  end;
  if Prepared and not Committed and Restored then RestartPrevious;
  if BackupDirectory <> '' then begin
    if Committed or (Prepared and Restored) then begin
      CheckParents(BackupDirectory);
      if not DelTree(BackupDirectory, True, True, True) then Log('Recovery copy cleanup incomplete: ' + BackupDirectory);
    end
    else
      SuppressibleMsgBox('The recovery copy was preserved at ' + BackupDirectory + '. Do not run a partial bundle; restore these files before retrying.', mbError, MB_OK, IDOK);
  end;
  if BackupFiles <> nil then BackupFiles.Free;
  if BackupHashes <> nil then BackupHashes.Free;
end;

function GetCustomSetupExitCode: Integer;
begin
  Result := 0;
  if PostInstallWarning then Result := 10;
end;

procedure CurUninstallStepChanged(Step: TUninstallStep);
var
  Receipt: String;
begin
  if Step = usAppMutexCheck then begin
    { Inno 7.1.1 calls this after confirmation and before any removal. Exceptions are fatal. }
    CheckInstallLocation;
    OfflineRecovery := not CheckIdentity;
    if not OfflineRecovery then PrepareProcess;
    LockBundle;
    if OfflineRecovery = CheckIdentity then
      RaiseException('The installed control executable changed during preparation. Cancel and retry.');
  end else if Step = usUninstall then begin
    if OfflineRecovery then RemoveOfflineStartup
    else begin
      Receipt := RunControl('service unregister-startup', True);
      if (Receipt <> 'KLICKY_STARTUP_V1 ABSENT') and (Receipt <> 'KLICKY_STARTUP_V1 FOREIGN') then
        RaiseException('Login shortcut removal was not confirmed. No bundle files were removed.');
    end;
    RemovalStarted := True;
  end else if Step = usPostUninstall then begin
    UnlockBundle;
    DeleteFile(ExpandConstant('{app}\') + MarkerName);
    RemoveDir(ExpandConstant('{app}'));
  end;
end;

procedure DeinitializeUninstall;
begin
  UnlockBundle;
  if not RemovalStarted then RestartPrevious;
end;
