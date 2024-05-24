use embed_manifest::embed_manifest_file;
use std::path::PathBuf;
use std::process::Command;
use {
    std::{env, io},
    winres::WindowsResource,
};

fn get_sdk_path() -> Option<String> {
    let mut sdk_path: Option<String> = None;
    let target = env::var("TARGET").unwrap();

    // For whatever reason, find_tool doesn't work directly on `midl.exe`. It
    // DOES work however, on `link.exe`, and will hand us back a PATH that has
    // the path to the appropriate midl.exe in it.
    let link_exe = cc::windows_registry::find_tool(target.as_str(), "link.exe")
        .expect("Failed to find link.exe");
    link_exe.env().iter().for_each(|(k, v)| {
        if k == "PATH" {
            let elements = (v.to_str().expect("path exploded"))
                .split(';')
                .collect::<Vec<&str>>();
            // iterate over the elements to find one that starts with
            // "C:\Program Files (x86)\Windows Kits\10\bin\10.0.*"
            for element in elements {
                if element.starts_with("C:\\Program Files (x86)\\Windows Kits\\10\\bin\\10.0.") {
                    sdk_path = Some(element.to_string());
                }
            }
        }
    });
    sdk_path
}

fn get_sdk_tool(sdk_path: &Option<String>, tool_name: &str) -> String {
    // seems like, in a VS tools prompt, midl.exe is in the path so the above
    // doesn't include the path. kinda weird but okay?
    let tool_path = match sdk_path {
        Some(s) => PathBuf::from(s)
            .join(tool_name)
            .to_str()
            .unwrap()
            .to_owned(),
        None => {
            // This is the case that happens when you run the build from a VS
            // tools prompt. In this case, the tool is already in the path, so
            // we can just get the absolute path to the exe using the windows
            // path search.

            let tool_path = which::which(tool_name).expect("Failed to find tool in path");
            tool_path.to_str().unwrap().to_owned()
        }
    };
    tool_path
}

fn build_rpc() {
    // Build our RPC library

    let sdk_path: Option<String> = get_sdk_path();
    let midl_path = get_sdk_tool(&sdk_path, "midl.exe");

    // Now, we need to get the path to the shared include directory, which is
    // dependent on the SDK version. We're gonna find it based on the midl we
    // already found.
    //
    // Our midl path is now something like:
    // C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\midl.exe
    //
    // We want to get the path to the shared include directory, which is like
    //
    // C:\Program Files (x86)\Windows Kits\10\Include\10.0.19041.0\shared
    //
    // (of course, the version number will change depending on the SDK version)
    // So, just take that path, remove two elements from the end, replace bin with Include, and add shared.
    let mut include_path = PathBuf::from(midl_path.clone());
    include_path.pop();
    include_path.pop();
    // now we're at C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0
    let copy_of_include_path = include_path.clone();
    let version = copy_of_include_path.file_name().unwrap().to_str().unwrap();
    include_path.pop();
    include_path.pop();
    // now we're at C:\Program Files (x86)\Windows Kits\10\
    include_path.push("Include");
    include_path.push(version);
    include_path.push("shared");

    println!("midl_path: {:?}", midl_path);

    let target = env::var("TARGET").unwrap();

    let cl_path =
        cc::windows_registry::find_tool(target.as_str(), "cl.exe").expect("Failed to find cl.exe");
    // add cl.exe to our path
    let mut path = env::var("PATH").unwrap();
    path.push(';');
    path.push_str(cl_path.path().parent().unwrap().to_str().unwrap());
    env::set_var("PATH", path);

    // Great! we've now finally got a path to midl.exe, and cl.exe is on the PATH

    // Now we can actually run midl.exe, to compile the IDL file. This will
    // generate a bunch of files in the OUT_DIR which we need to do RPC.

    let mut cmd = Command::new(midl_path);
    cmd.arg("../cpp/rpc/sudo_rpc.idl");
    cmd.arg("/h").arg("sudo_rpc.h");
    cmd.arg("/target").arg("NT100"); // LOAD BEARING: Enables system_handle
    cmd.arg("/acf").arg("../cpp/rpc/sudo_rpc.acf");
    cmd.arg("/prefix").arg("client").arg("client_");
    cmd.arg("/prefix").arg("server").arg("server_");
    cmd.arg("/oldnames");
    cmd.arg("/I").arg(include_path);

    // Force midl to use the right architecture depending on our Rust target.
    cmd.arg("/env").arg(match target.as_str() {
        "x86_64-pc-windows-msvc" => "x64",
        "i686-pc-windows-msvc" => "win32",
        "aarch64-pc-windows-msvc" => "arm64",
        _ => panic!("Unknown target {}", target),
    });

    // I was pretty confident that we needed to pass /protocol ndr64 here, but
    // if we do that it'll break the win32 build. Omitting it entirely seems to
    // Just Work.
    // cmd.arg("/protocol").arg("ndr64");

    cmd.arg("/out").arg(env::var("OUT_DIR").unwrap());

    println!("Full midl command: {:?}", cmd);
    let mut midl_result = cmd.spawn().expect("Failed to run midl.exe");
    println!(
        "midl.exe returned {:?}",
        midl_result.wait().expect("midl.exe failed")
    );

    // Now that our PRC header and stubs were generated, we can compile them
    // into our actual RPC lib.
    let mut rpc_build = cc::Build::new();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    rpc_build
        .warnings(true)
        .warnings_into_errors(true)
        .include(env::var("OUT_DIR").unwrap())
        .file(out_dir.join("sudo_rpc_c.c"))
        .file(out_dir.join("sudo_rpc_s.c"))
        .file("../cpp/rpc/RpcClient.c")
        .flag("/guard:ehcont");

    println!("build cmdline: {:?}", rpc_build.get_compiler().to_command());
    rpc_build.compile("myRpc");
    println!("cargo:rustc-link-lib=myRpc");

    println!("cargo:rerun-if-changed=../cpp/rpc/RpcClient.c");
    println!("cargo:rerun-if-changed=../cpp/rpc/sudo_rpc.idl");
}

fn build_logging() {
    // Build our Event Logging library

    let sdk_path: Option<String> = get_sdk_path();
    let mc_path = get_sdk_tool(&sdk_path, "mc.exe");

    println!("mc_path: {:?}", mc_path);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut cmd = Command::new(mc_path);
    cmd.arg("-h").arg(&out_dir);
    cmd.arg("-r").arg(&out_dir);
    cmd.arg("../cpp/logging/instrumentation.man");

    println!("Full mc command: {:?}", cmd);

    let mc_result = cmd
        .spawn()
        .expect("Failed to run mc.exe")
        .wait()
        .expect("mc.exe failed");
    if !mc_result.success() {
        eprintln!("\n\nerror occurred: {}\n\n", mc_result);
        std::process::exit(1);
    }

    let mut logging_build = cc::Build::new();
    logging_build
        .warnings(true)
        .warnings_into_errors(true)
        .include(env::var("OUT_DIR").unwrap())
        .file("../cpp/logging/EventViewerLogging.c")
        .flag("/guard:ehcont");

    println!(
        "build cmdline: {:?}",
        logging_build.get_compiler().to_command()
    );
    logging_build.compile("myEventLogging");
    println!("cargo:rustc-link-lib=myEventLogging");

    println!("cargo:rerun-if-changed=../cpp/logging/EventViewerLogging.c");
    println!("cargo:rerun-if-changed=../cpp/logging/instrumentation.man");
}

fn main() -> io::Result<()> {
    // Always build the RPC lib.
    build_rpc();

    // Always build the Event Logging lib.
    build_logging();

    println!("cargo:rerun-if-changed=sudo/Resources/en-US/Sudo.resw");
    println!("cargo:rerun-if-changed=sudo.rc");
    println!("cargo:rerun-if-changed=../Generated Files/out.rc");
    println!("cargo:rerun-if-changed=../Generated Files/out_resources.h");

    // compile the resource file.
    // Run
    // powershell -c .pipelines\convert-resx-to-rc.ps1 .\ no_existy.h res.h no_existy.rc out.rc resource_ids.rs
    // to generate the resources

    let generate_resources_result = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-c")
        .arg("..\\.pipelines\\convert-resx-to-rc.ps1")
        .arg("..\\") // Root directory which contains the resx files
        .arg("no_existy.h") // File name of the base resource.h which contains all the non-localized resource definitions
        .arg("resource.h") // Target file name of the resource header file, which will be used in code - Example: resource.h
        .arg("sudo\\sudo.rc") // File name of the base ProjectName.rc which contains all the non-localized resources
        .arg("out.rc") // Target file name of the resource rc file, which will be used in code - Example: ProjectName.rc
        .arg("resource_ids.rs") // Target file name of the rust resource file, which will be used in code - Example: resource.rs
        .status()
        .expect("Failed to generate resources");

    if !generate_resources_result.success() {
        println!(
            "\nFailed to generate resources by executing powershell script: {}.",
            generate_resources_result
        );
        println!(
            "Maybe you haven't granted the access to execute the powershell script on this system."
        );
        println!("For more details, please execute the `cargo build` command with the `-vv` flag.");
        std::process::exit(1);
    }

    // witchcraft to get windows.h from the SDK to be able to be found, for the resource compiler
    let target = env::var("TARGET").unwrap();
    if let Some(tool) = cc::windows_registry::find_tool(target.as_str(), "cl.exe") {
        for (key, value) in tool.env() {
            env::set_var(key, value);
        }
    }

    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // TODO:MSFT
        // Re-add the following:
        //     <windowsSettings>
        //       <consoleAllocationPolicy xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>
        //     </windowsSettings>
        // to our manifest
        embed_manifest_file("sudo.manifest").expect("Failed to embed manifest");

        let generated_rc_content = std::fs::read_to_string("../Generated Files/out.rc").unwrap();
        let instrumentation_rc_content =
            std::fs::read_to_string(env::var("OUT_DIR").unwrap() + "/instrumentation.rc").unwrap();
        let generated_header = std::fs::read_to_string("../Generated Files/resource.h").unwrap();
        WindowsResource::new()
            // We don't want to use set_resource_file here, because we _do_ want
            // the file version info that winres can autogenerate. Instead,
            // manually stitch in our generated header (with resource IDs), and
            // our generated rc file (with the localized strings)
            .append_rc_content(&generated_header)
            .append_rc_content(&instrumentation_rc_content)
            .append_rc_content(&generated_rc_content)
            .compile()?;
    }
    Ok(())
}

// Magic incantation to get the build to generate the .rc file, before we build things:
//
// powershell -c .pipelines\convert-resx-to-rc.ps1 src\cascadia\ this_doesnt_exist.h out_resources.h no_existy.rc out.rc resource_ids.rs
//
// do that from the root of the repo, and it will generate the .rc file, into
// src\cascadia\Generated Files\{out.rc, out_resources.h}
//
//
// Alternatively,
//  powershell -c .pipelines\convert-resx-to-rc.ps1 .\ no_existy.h res.h no_existy.rc out.rc resource_ids.rs
//
// will generate the .rc file into the a "Generated Files" dir in the root of the repo.
