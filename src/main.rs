use itertools::Itertools;
use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{Bytecode, FunctionDefinition, SignatureToken},
    views::FunctionDefinitionView,
    CompiledModule,
};
// use move_core_types::language_storage::ModuleId;
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs::File;
use std::hash::Hash;
use std::io::{BufReader, Read};

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    let f = File::open(&args[1]).unwrap();
    let mut reader = BufReader::new(f);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).unwrap();
    let cm = CompiledModule::deserialize(&buffer).unwrap();
    let ff = cm
        .function_defs
        .iter()
        .filter_map(|f| inspect_function(&cm, f).ok())
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&ff).unwrap());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Func {
    name: String,
    visibility: String,
    is_entry: bool,
    generic_type_params: Vec<String>,
    params: Vec<String>,
    #[serde(rename(serialize = "return"))]
    ret: Vec<String>,
    read_resources: Vec<Resource>,
    write_resources: Vec<Resource>,
    called_functions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Resource {
    module_addr: String,
    module_name: String,
    resource_name: String,
}

pub fn inspect_function(cm: &CompiledModule, func: &FunctionDefinition) -> Result<Func, ()> {
    let view = &BinaryIndexedView::Module(&cm);
    let r = extract(func, |b| match b {
        Bytecode::ImmBorrowGlobal(a) => {
            let h = view.struct_def_at(a.clone()).unwrap().struct_handle.clone();
            let h = view.struct_handle_at(h);
            let m = view.module_handle_at(h.module);
            let m = view.module_id_for_handle(m);
            Some(Resource {
                module_addr: format!("0x{}", m.address()),
                module_name: m.name().to_string(),
                resource_name: view.identifier_at(h.name).to_string(),
            })
        }
        _ => None,
    });
    let w = extract(func, |b| match b {
        Bytecode::MutBorrowGlobal(a) => {
            let h = view.struct_def_at(a.clone()).unwrap().struct_handle.clone();
            let h = view.struct_handle_at(h);
            let m = view.module_handle_at(h.module);
            let m = view.module_id_for_handle(m);
            Some(Resource {
                module_addr: format!("0x{}", m.address()),
                module_name: m.name().to_string(),
                resource_name: view.identifier_at(h.name).to_string(),
            })
        }
        _ => None,
    });
    let c = extract(func, |b| match b {
        Bytecode::Call(a) => {
            let h = view.function_handle_at(a.clone());
            let m = view.module_handle_at(h.module);
            Some(format!(
                "0x{}::{}",
                view.module_id_for_handle(m),
                view.identifier_at(h.name).clone()
            ))
        }
        _ => None,
    });
    let f = FunctionDefinitionView::new(cm, func);
    let ret = f
        .return_()
        .0
        .iter()
        .map(|d| format_signature_token(d))
        .collect::<Vec<_>>();
    let params = f
        .parameters()
        .0
        .iter()
        .map(|d| format_signature_token(d))
        .collect::<Vec<_>>();
    Ok(Func {
        name: f.name().to_string(),
        visibility: format!("{:?}", func.visibility),
        is_entry: func.is_entry,
        generic_type_params: vec![],
        params,
        ret,
        read_resources: r,
        write_resources: w,
        called_functions: c,
    })
}

fn format_signature_token(token: &SignatureToken) -> String {
    match token {
        SignatureToken::Bool => "Bool".to_string(),
        SignatureToken::U8 => "U8".to_string(),
        SignatureToken::U64 => "U64".to_string(),
        SignatureToken::U128 => "U128".to_string(),
        SignatureToken::Address => "Address".to_string(),
        SignatureToken::Signer => "Signer".to_string(),
        SignatureToken::Vector(boxed) => format!("Vector({})", format_signature_token(&*boxed)),
        SignatureToken::Struct(idx) => format!("Struct({:?})", idx),
        SignatureToken::StructInstantiation(idx, types) => {
            format!("StructInstantiation({:?}, {:?})", idx, types)
        }
        SignatureToken::Reference(boxed) => {
            format!("Reference({})", format_signature_token(&*boxed))
        }
        SignatureToken::MutableReference(boxed) => {
            format!("MutableReference({})", format_signature_token(&*boxed))
        }
        SignatureToken::TypeParameter(idx) => format!("TypeParameter({:?})", idx),
    }
}

fn extract<F, R>(func: &FunctionDefinition, f: F) -> Vec<R>
where
    F: Fn(&Bytecode) -> Option<R>,
    R: Clone + Hash + Ord + PartialOrd + Eq + PartialEq,
{
    func.code
        .as_ref()
        .map(|c| {
            c.code
                .iter()
                .filter_map(|b| f(b))
                .sorted_by(|a, b| Ord::cmp(&a, &b))
                .unique()
                .collect::<Vec<_>>()
        })
        .unwrap()
}
