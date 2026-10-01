//! Read-only process and OOXML evidence. Native APIs replace external interpreters.
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::{fs::File, io::Read};
use windows::{
    Data::Xml::Dom::{IXmlNode, XmlDocument, XmlLoadSettings},
    Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, HANDLE, SYSTEMTIME},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Threading::{
                GetProcessTimes, OpenProcess, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
            Time::FileTimeToSystemTime,
            Variant::VARIANT,
            Wmi::{
                IWbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WbemLocator,
            },
        },
    },
    core::{BSTR, HSTRING, Interface, w},
};
const LIMIT: u64 = 4 * 1024 * 1024;
fn winrt_runtime() -> Result<(), CtlError> {
    // windows-rs caches agile WinRT factories for the process lifetime. Keep the
    // MTA alive for that same lifetime, including between repeated evidence calls.
    static RUNTIME: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            // Safety: acquire one process-lifetime COM runtime lease, no borrowed pointers.
            unsafe { windows::Win32::System::Com::CoIncrementMTAUsage() }
                .map(|cookie| {
                    let _process_lifetime_cookie = cookie;
                })
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map(|_| ())
        .map_err(err)
}
pub fn sha256(bytes: &[u8]) -> Result<String, CtlError> {
    use windows::Security::Cryptography::{Core::HashAlgorithmProvider, CryptographicBuffer};
    winrt_runtime()?;
    let buffer = CryptographicBuffer::CreateFromByteArray(bytes).map_err(err)?;
    let algorithm = HashAlgorithmProvider::OpenAlgorithm(&HSTRING::from("SHA256")).map_err(err)?;
    let digest = algorithm.HashData(&buffer).map_err(err)?;
    Ok(CryptographicBuffer::EncodeToHexString(&digest)
        .map_err(err)?
        .to_string())
}
fn err(e: impl std::fmt::Display) -> CtlError {
    CtlError::new(
        ErrorCode::NotActionable,
        format!("native evidence unavailable: {e}"),
    )
}
struct Com(bool);
impl Com {
    fn enter() -> Result<Self, CtlError> {
        // This thread may already have a UIA apartment; do not uninitialize that apartment.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Self(true))
        } else if result.0 == 0x80010106u32 as i32 {
            Ok(Self(false))
        } else {
            Err(err(result))
        }
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() }
        }
    }
}
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Uses a live process handle for identity; metadata is optional during shutdown.
pub fn process(pid: u32, metadata: bool) -> Result<Value, CtlError> {
    if pid == 0 {
        return Err(CtlError::protocol("process ID must be positive"));
    }
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => Process(h),
        Err(e) if e.code() == ERROR_INVALID_PARAMETER.to_hresult() => {
            return Ok(json!({"state":"absent"}));
        }
        Err(e) => return Err(err(e)),
    };
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .map_err(err)?;
    if exited.dwLowDateTime != 0 || exited.dwHighDateTime != 0 {
        return Ok(json!({"state":"absent"}));
    }
    let mut date = SYSTEMTIME::default();
    unsafe { FileTimeToSystemTime(&created, &mut date) }.map_err(err)?;
    let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    // Match the existing UTC instance representation (CIM microsecond precision).
    let start = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:07}Z",
        date.wYear,
        date.wMonth,
        date.wDay,
        date.wHour,
        date.wMinute,
        date.wSecond,
        (ticks % 10_000_000) / 10 * 10
    );
    let created_unix_ms = (((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
        .saturating_sub(116444736000000000)
        / 10000;
    if !metadata {
        return Ok(json!({"start":start,"created_unix_ms":created_unix_ms}));
    }
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut length,
        )
    }
    .map_err(err)?;
    let program = String::from_utf16(&path[..length as usize]).map_err(err)?;
    let _com = Com::enter()?;
    let locator: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }.map_err(err)?;
    let empty = BSTR::new();
    let service = unsafe {
        locator.ConnectServer(
            &BSTR::from("ROOT\\CIMV2"),
            &empty,
            &empty,
            &empty,
            0,
            &empty,
            None,
        )
    }
    .map_err(err)?;
    // Authenticate only this local WMI proxy; do not alter process-wide COM security.
    unsafe {
        windows::Win32::System::Com::CoSetProxyBlanket(
            &service,
            10,
            0,
            None,
            windows::Win32::System::Com::RPC_C_AUTHN_LEVEL_CALL,
            windows::Win32::System::Com::RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            windows::Win32::System::Com::EOAC_NONE,
        )
    }
    .map_err(err)?;
    let query = BSTR::from(format!(
        "SELECT CommandLine FROM Win32_Process WHERE ProcessId={pid}"
    ));
    let rows = unsafe {
        service.ExecQuery(
            &BSTR::from("WQL"),
            &query,
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        )
    }
    .map_err(err)?;
    let mut items = [None];
    let mut returned = 0;
    unsafe { rows.Next(5000, &mut items, &mut returned) }
        .ok()
        .map_err(err)?;
    let row = items[0]
        .as_ref()
        .filter(|_| returned == 1)
        .ok_or_else(|| err("process metadata unavailable"))?;
    let mut value = VARIANT::default();
    unsafe { row.Get(w!("CommandLine"), 0, &mut value, None, None) }.map_err(err)?;
    let command = BSTR::try_from(&value).map_err(err)?.to_string();
    if command.is_empty() {
        return Err(err("empty process command line"));
    }
    // Holding the original handle prevents trusting metadata after that instance exits.
    unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .map_err(err)?;
    if exited.dwLowDateTime != 0 || exited.dwHighDateTime != 0 {
        return Err(err("process exited during metadata query"));
    }
    Ok(
        json!({"start":start,"program":program,"command_line":command,"created_unix_ms":created_unix_ms}),
    )
}

fn nodes(node: &IXmlNode, xpath: &str) -> Result<Vec<IXmlNode>, CtlError> {
    let list = node.SelectNodes(&HSTRING::from(xpath)).map_err(err)?;
    (0..list.Length().map_err(err)?)
        .map(|i| list.Item(i).map_err(err))
        .collect()
}
fn text(node: &IXmlNode, xpath: &str) -> Result<String, CtlError> {
    let mut output = String::new();
    for n in nodes(node, xpath)? {
        output.push_str(&n.InnerText().map_err(err)?.to_string());
    }
    Ok(output)
}
fn xml(zip: &mut zip::ZipArchive<File>, name: &str) -> Result<IXmlNode, CtlError> {
    let part = zip.by_name(name).map_err(err)?;
    if part.size() > LIMIT {
        return Err(err("XML part exceeds 4 MiB"));
    }
    let mut bytes = Vec::new();
    part.take(LIMIT + 1).read_to_end(&mut bytes).map_err(err)?;
    if bytes.len() as u64 > LIMIT {
        return Err(err("XML part exceeds 4 MiB"));
    }
    let content = if bytes.starts_with(&[255, 254]) || bytes.starts_with(&[254, 255]) {
        if bytes.len() % 2 != 0 {
            return Err(err("invalid UTF-16 XML"));
        }
        let little = bytes[0] == 255;
        let words: Vec<_> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if little {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect();
        String::from_utf16(&words).map_err(err)?
    } else {
        String::from_utf8(bytes).map_err(err)?
    };
    let settings = XmlLoadSettings::new().map_err(err)?;
    settings.SetProhibitDtd(true).map_err(err)?;
    settings.SetResolveExternals(false).map_err(err)?;
    settings.SetMaxElementDepth(128).map_err(err)?;
    let doc = XmlDocument::new().map_err(err)?;
    doc.LoadXmlWithSettings(
        &HSTRING::from(content.trim_start_matches('\u{feff}')),
        &settings,
    )
    .map_err(err)?;
    doc.cast().map_err(err)
}
pub fn office(path: &str, kind: &str) -> Result<Value, CtlError> {
    winrt_runtime()?;
    let file = File::open(path).map_err(err)?;
    if file.metadata().map_err(err)?.len() > LIMIT {
        return Err(err("Office package exceeds 4 MiB"));
    }
    let mut zip = zip::ZipArchive::new(file).map_err(err)?;
    let result = match kind {
        "docx" => {
            let doc = xml(&mut zip, "word/document.xml")?;
            let paragraphs = nodes(&doc, "//*[local-name()='body']//*[local-name()='p']")?
                .iter()
                .map(|p| text(p, ".//*[local-name()='t']"))
                .collect::<Result<Vec<_>, _>>()?;
            let runs = nodes(
                &doc,
                "//*[local-name()='body']//*[local-name()='r'][*[local-name()='t']]",
            )?;
            let mut bold = !runs.is_empty();
            for run in runs {
                let bs = nodes(&run, "./*[local-name()='rPr']/*[local-name()='b']")?;
                if bs.is_empty() {
                    bold = false;
                } else {
                    let v = text(&bs[0], "@*[local-name()='val']")?;
                    if ["0", "false", "off"].contains(&v.as_str()) {
                        bold = false;
                    }
                }
            }
            json!({"text":paragraphs.join("\n"),"bold":bold})
        }
        "xlsx" => {
            let mut strings = vec![];
            if zip.file_names().any(|n| n == "xl/sharedStrings.xml") {
                let shared = xml(&mut zip, "xl/sharedStrings.xml")?;
                for n in nodes(&shared, "//*[local-name()='si']")? {
                    strings.push(text(&n, ".//*[local-name()='t']")?);
                }
            }
            let sheet = xml(&mut zip, "xl/worksheets/sheet1.xml")?;
            let mut result = serde_json::Map::new();
            for cell in nodes(&sheet, "//*[local-name()='c']")? {
                let address = text(&cell, "@r")?;
                let kind = text(&cell, "@t")?;
                let mut value = text(&cell, "./*[local-name()='v']")?;
                if kind == "s" {
                    let index = value.parse::<usize>().map_err(err)?;
                    value = strings
                        .get(index)
                        .ok_or_else(|| err("bad shared string index"))?
                        .clone();
                } else if kind == "inlineStr" {
                    value = text(&cell, ".//*[local-name()='t']")?;
                }
                if address.is_empty() || result.contains_key(&address) {
                    return Err(err("missing or duplicate cell address"));
                }
                result.insert(
                    address,
                    json!({"value":value,"formula":text(&cell,"./*[local-name()='f']")?}),
                );
            }
            Value::Object(result)
        }
        _ => return Err(CtlError::protocol("unsupported Office evidence kind")),
    };
    if serde_json::to_vec(&result).map_err(err)?.len() as u64 > LIMIT {
        return Err(err("Office evidence exceeds limit"));
    }
    Ok(result)
}

/// 父进程 PID(ToolHelp 快照;None = 查不到/已退出)。verify-owner 进程树
/// 归属用:保存/下载对话框常跑在 contentproc 等子进程,命令行不含任务令牌
/// (zotero 实证)——沿父链找令牌。
pub fn parent_pid(pid: u32) -> Option<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut first = Process32FirstW(snapshot, &mut entry).is_ok();
        while first {
            if entry.th32ProcessID == pid {
                let _ = windows::Win32::Foundation::CloseHandle(snapshot);
                return (entry.th32ParentProcessID != 0).then_some(entry.th32ParentProcessID);
            }
            first = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn package(parts: &[(&str, &[u8])]) -> std::path::PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "office-{}.zip",
            actl_core::snapshot::new_snapshot_id()
        ));
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in parts {
            zip.start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }
    #[test]
    fn office_paragraphs_bold_shared_strings_and_formula() {
        let doc=package(&[("word/document.xml",br#"<w:document xmlns:w="urn:test"><w:body><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>one</w:t></w:r></w:p><w:p><w:r><w:rPr><w:b w:val="true"/></w:rPr><w:t>two</w:t></w:r></w:p></w:body></w:document>"#)]);
        for _ in 0..3 {
            assert_eq!(
                office(doc.to_str().unwrap(), "docx").unwrap(),
                json!({"text":"one\ntwo","bold":true})
            );
        }
        let sheet=package(&[("xl/sharedStrings.xml",b"<sst><si><r><t>hello</t></r><r><t> world</t></r></si></sst>"),("xl/worksheets/sheet1.xml",b"<worksheet><c r='A1' t='s'><v>0</v></c><c r='B1'><f>SUM(1,2)</f><v>3</v></c><c r='C1' t='inlineStr'><is><t>inline</t></is></c></worksheet>")]);
        let result = office(sheet.to_str().unwrap(), "xlsx").unwrap();
        assert_eq!(result["A1"]["value"], "hello world");
        assert_eq!(result["B1"]["formula"], "SUM(1,2)");
        assert_eq!(result["C1"]["value"], "inline");
        std::fs::remove_file(doc).unwrap();
        std::fs::remove_file(sheet).unwrap();
    }
    #[test]
    fn office_rejects_dtd_duplicate_cells_invalid_indices_and_expansion() {
        for xml in [
            "<!DOCTYPE x [<!ENTITY value 'secret'>]><worksheet><c r='A1'><v>&value;</v></c></worksheet>",
            "<worksheet><c r='A1'/><c r='A1'/></worksheet>",
            "<worksheet><c r='A1' t='s'><v>9</v></c></worksheet>",
            "<worksheet><c/></worksheet>",
        ] {
            let path = package(&[("xl/worksheets/sheet1.xml", xml.as_bytes())]);
            assert!(office(path.to_str().unwrap(), "xlsx").is_err());
            std::fs::remove_file(path).unwrap();
        }
        let large = vec![b' '; LIMIT as usize + 1];
        let path = package(&[("word/document.xml", &large)]);
        assert!(office(path.to_str().unwrap(), "docx").is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn sha256_known_vector() {
        let first = sha256(b"first").unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(
            sha256(b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn identity_without_metadata_and_current_process_metadata_agree() {
        let pid = std::process::id();
        let identity = process(pid, false).unwrap();
        assert!(identity.get("command_line").is_none());
        let full = process(pid, true).unwrap();
        assert_eq!(identity["start"], full["start"]);
        assert!(full["command_line"].as_str().unwrap().contains("actl_uia"));
    }
    #[test]
    fn exit_is_distinct_from_unknown() {
        assert_eq!(process(u32::MAX, false).unwrap()["state"], "absent");
        assert!(process(0, false).is_err());
    }
}
