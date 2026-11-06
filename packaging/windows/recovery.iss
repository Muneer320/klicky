{ Offline removal for an installer-owned bundle whose CLI is absent.
  The caller holds the exclusive bundle lease before entering this code.
  Keep shortcut ownership rules aligned with src/windows/startup.rs. }

type
  IRecoveryShellLink = interface(IUnknown)
    '{000214F9-0000-0000-C000-000000000046}'
    procedure GetPath(Buffer: String; Count: Integer; FindData: NativeUInt; Flags: LongWord); safecall;
    procedure GetIDListUnused;
    procedure SetIDListUnused;
    procedure GetDescription(Buffer: String; Count: Integer); safecall;
    procedure SetDescriptionUnused;
    procedure GetWorkingDirectory(Buffer: String; Count: Integer); safecall;
    procedure SetWorkingDirectoryUnused;
    procedure GetArguments(Buffer: String; Count: Integer); safecall;
    procedure SetArgumentsUnused;
    function GetHotkey: Word; safecall;
    procedure SetHotkeyUnused;
    function GetShowCmd: Integer; safecall;
    procedure SetShowCmdUnused;
    procedure GetIconLocation(Buffer: String; Count: Integer; out Index: Integer); safecall;
  end;
  IRecoveryPersistFile = interface(IUnknown)
    '{0000010B-0000-0000-C000-000000000046}'
    procedure GetClassIDUnused;
    procedure IsDirtyUnused;
    procedure Load(Name: String; Mode: LongWord); safecall;
  end;
  IRecoveryLinkData = interface(IUnknown)
    '{45E2B4AE-B1C3-11D0-B92F-00A0C90312E1}'
    procedure AddDataBlockUnused;
    procedure CopyDataBlockUnused;
    procedure RemoveDataBlockUnused;
    function GetFlags: LongWord; safecall;
  end;
  TRecoveryAttributes = record
    Attributes, ReparseTag: LongWord;
  end;

function GetFileInformationByHandleEx(Handle: THandle; Class_: Integer;
  var Info: TRecoveryAttributes; Size: LongWord): Boolean;
  external 'GetFileInformationByHandleEx@kernel32.dll stdcall';
function ReadFile(Handle: THandle; Buffer: AnsiString; Count: LongWord;
  var ReadCount: LongWord; Overlapped: NativeUInt): Boolean;
  external 'ReadFile@kernel32.dll stdcall';
function SetFileInformationByHandle(Handle: THandle; Class_: Integer;
  var DeleteOnClose: Byte; Size: LongWord): Boolean;
  external 'SetFileInformationByHandle@kernel32.dll stdcall';

procedure CheckRecoveryHandle(Handle: THandle);
var
  Info: TRecoveryAttributes;
begin
  if not GetFileInformationByHandleEx(Handle, 9, Info, SizeOf(Info)) then
    RaiseException('Cannot inspect the recovery file: ' + SysErrorMessage(DLLGetLastError));
  if (Info.Attributes and (ReparsePoint or DirectoryAttribute)) <> 0 then
    RaiseException('Offline recovery requires an ordinary file; links are preserved.');
end;

function RecoveryText(const Buffer: String): String;
var
  Terminator: Integer;
begin
  Terminator := Pos(#0, Buffer);
  if Terminator = 0 then RaiseException('Unterminated shortcut field; shortcut preserved.');
  Result := Copy(Buffer, 1, Terminator - 1);
end;

function OwnsRecoveryShortcut(const Bytes: AnsiString): Boolean;
var
  Temporary, Description, Arguments, Target, Directory, Icon: String;
  Obj: IUnknown;
  Link: IRecoveryShellLink;
  Persist: IRecoveryPersistFile;
  Data: IRecoveryLinkData;
  IconIndex: Integer;
begin
  Result := False;
  Temporary := GenerateUniqueName(ExpandConstant('{tmp}'), '.lnk');
  try
    if not SaveStringToFile(Temporary, Bytes, False) then
      RaiseException('Cannot stage the bounded shortcut for inspection.');
    Obj := CreateComObject(StringToGUID('{00021401-0000-0000-C000-000000000046}'));
    Persist := IRecoveryPersistFile(Obj);
    Persist.Load(Temporary, 0);
    Link := IRecoveryShellLink(Obj);
    Data := IRecoveryLinkData(Obj);
    { Reject custom execution, environment, advertised and chained links. }
    if (Data.GetFlags and $02827600) <> 0 then Exit;
    SetLength(Description, 512);
    SetLength(Arguments, 32768);
    SetLength(Target, 32768);
    SetLength(Directory, 32768);
    SetLength(Icon, 32768);
    Link.GetDescription(Description, Length(Description));
    Link.GetArguments(Arguments, Length(Arguments));
    Link.GetPath(Target, Length(Target), 0, 4); { SLGP_RAWPATH, never Resolve }
    Link.GetWorkingDirectory(Directory, Length(Directory));
    Link.GetIconLocation(Icon, Length(Icon), IconIndex);
    Result := (RecoveryText(Description) = 'Klicky per-user login startup; owner klicky.windows.v1') and
      (RecoveryText(Arguments) = '') and (RecoveryText(Icon) = '') and
      (IconIndex = 0) and (Link.GetHotkey = 0) and (Link.GetShowCmd = SW_SHOWNORMAL) and
      (RecoveryText(Target) = ExpandConstant('{app}\klicky-desktop.exe')) and
      (RecoveryText(Directory) = ExpandConstant('{app}'));
  finally
    Data := nil;
    Link := nil;
    Persist := nil;
    Obj := nil;
    DeleteFile(Temporary);
  end;
end;

procedure RemoveOfflineStartup;
var
  Path, LockPath: String;
  Writer, Shortcut: THandle;
  Size, High, ReadCount: LongWord;
  Bytes: AnsiString;
  DeleteOnClose: Byte;
  Owned: Boolean;
begin
  Path := ExpandConstant('{userstartup}\Klicky.lnk');
  CheckParents(ExtractFileDir(Path));
  if AttributesIfPresent(Path) = InvalidAttributes then Exit;
  LockPath := ExpandConstant('{localappdata}\Klicky\startup.lock');
  CheckParents(ExtractFileDir(LockPath));
  if not ForceDirectories(ExtractFileDir(LockPath)) then
    RaiseException('Cannot prepare the startup ownership lock.');
  Writer := CreateFileW(LockPath, GenericReadWrite, 0, 0, OpenAlways, OpenReparsePoint, 0);
  if Writer = THandle(-1) then RaiseException('Another startup operation is active. Retry removal when it finishes.');
  try
    CheckRecoveryHandle(Writer);
    { Hold the inspected object against writes. Delete that object by handle,
      so a replacement at the old name can never be removed accidentally. }
    Shortcut := CreateFileW(Path, $80010000, 5, 0, 3, OpenReparsePoint, 0);
    if Shortcut = THandle(-1) then RaiseException('Cannot inspect the login shortcut safely. No application files were removed.');
    try
      CheckRecoveryHandle(Shortcut);
      High := 0;
      Size := GetFileSize(Shortcut, High);
      if (High <> 0) or (Size > 65536) then RaiseException('Startup shortcut exceeds 64 KiB; preserved.');
      SetLength(Bytes, Size);
      if not ReadFile(Shortcut, Bytes, Size, ReadCount, 0) or (ReadCount <> Size) then
        RaiseException('Cannot read the startup shortcut; preserved.');
      try
        Owned := OwnsRecoveryShortcut(Bytes);
      except
        Log('Unrecognized startup shortcut preserved: ' + GetExceptionMessage);
        Owned := False;
      end;
      if not Owned then begin
        Log('Edited or foreign startup shortcut preserved.');
        Exit;
      end;
      DeleteOnClose := 1;
      if not SetFileInformationByHandle(Shortcut, 4, DeleteOnClose, 1) then
        RaiseException('Cannot remove the owned startup shortcut. No application files were removed.');
      Log('Removed owned startup shortcut without the installed CLI.');
    finally
      CloseHandle(Shortcut);
    end;
  finally
    CloseHandle(Writer);
  end;
end;
