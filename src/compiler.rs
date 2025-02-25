use std::collections::HashMap;
use std::error::Error;
use std::process::exit;

use crate::asm_generator::x86_64_nasm_generator;
use crate::parser::block::Block;
use crate::parser::expr::{CompareOp, Expr, FunctionCall, Op, UnaryExpr};
use crate::parser::function::{Function, FunctionArg};
use crate::parser::parse_file;
use crate::parser::program::ProgramItem;
use crate::parser::stmt::{
    Assgin, AssginOp, ElseBlock, IFStmt, Stmt, VariableDeclare, VariableType, WhileStmt,
};

macro_rules! asm {
    ($($arg:tt)+) => (
        format!("    {}\n",format_args!($($arg)+))
    );
}

pub fn compile_to_asm(path: String) {
    let mut compiler = Compiler::new();
    let (instr_buf, data_buf) = compiler
        .compile(path.clone())
        .expect("Can not Compile Program");
    x86_64_nasm_generator(path, instr_buf, data_buf).unwrap();
}

pub fn mem_word(size: usize) -> String {
    match size {
        1 => "byte".to_string(),
        2 => "word".to_string(),
        4 => "dword".to_string(),
        8 => "qword".to_string(),
        _ => {
            unreachable!("Incurrect Size")
        }
    }
}

pub fn rbs(register: &str, size: usize) -> String {
    match register {
        "a" | "b" | "c" | "d" => match size {
            1 => format!("{register}l"),
            2 => format!("{register}x"),
            4 => format!("e{register}x"),
            8 => format!("r{register}x"),
            _ => {
                unreachable!("Incurrect Size")
            }
        },
        "sp" | "bp" => match size {
            1 => format!("{register}l"),
            2 => register.to_string(),
            4 => format!("e{register}"),
            8 => format!("r{register}"),
            _ => {
                unreachable!("Incurrect Size")
            }
        },
        "si" | "di" => match size {
            1 => format!("{register}l"),
            2 => register.to_string(),
            4 => format!("e{register}"),
            8 => format!("r{register}"),
            _ => {
                unreachable!("Incurrect Size")
            }
        },
        "r8" | "r9" | "r10" | "r11" => match size {
            1 => format!("{register}b"),
            2 => format!("{register}w"),
            4 => format!("{register}d"),
            8 => register.to_string(),
            _ => {
                unreachable!("Incurrect Size")
            }
        },
        _ => {
            panic!("Wrong register identifier!");
        }
    }
}

pub fn function_args_register(arg_numer: usize, size: usize) -> String {
    match arg_numer {
        0 => rbs("di", size),
        1 => rbs("si", size),
        2 => rbs("d", size),
        3 => rbs("c", size),
        4 => rbs("r8", size),
        5 => rbs("r9", size),
        _ => unreachable!(),
    }
}

#[derive(Debug, Clone)]
pub struct VariableMap {
    _ident: String,
    offset: usize,
    size: usize,
    item_size: usize,
    is_mut: bool,
}

pub struct Compiler {
    instruct_buf: Vec<String>,
    data_buf: Vec<String>,
    scoped_blocks: Vec<usize>,
    block_id: usize,
    variables_map: HashMap<String, VariableMap>,
    functions_map: HashMap<String, Function>,
    mem_offset: usize,
}

impl Compiler {
    // TODO: handle Error for Parsing
    pub fn new() -> Self {
        Self {
            instruct_buf: Vec::new(),
            data_buf: Vec::new(),
            scoped_blocks: Vec::new(),
            block_id: 0,
            variables_map: HashMap::new(),
            functions_map: HashMap::new(),
            mem_offset: 0,
        }
    }

    fn frame_size(&self) -> usize {
        2 << self.mem_offset.ilog2() as usize
    }

    pub fn find_variable(&self, ident: String) -> Option<VariableMap> {
        for block_id in &self.scoped_blocks {
            let map_ident = format!("{ident}%{}", block_id);
            let map = self.variables_map.get(&map_ident);
            if let Some(map) = map {
                return Some(map.clone());
            }
        }
        None
    }

    pub fn insert_variable(&mut self, var: &VariableDeclare) {
        let ident: String;
        let var_map: VariableMap;
        let mut size = 8;
        let mut item_size = 8;
        if var.v_type.is_some() {
            let typ = var.v_type.clone().unwrap();
            if let VariableType::Array(a, s) = typ {
                match *a.as_ref() {
                    VariableType::Int => {
                        size = 8 * s;
                        item_size = 8;
                    }
                    VariableType::Char => {
                        size = s;
                        item_size = 1;
                    }
                    _ => {
                        todo!("Unsuported Array Type");
                    }
                }
            }
        }
        if var.is_static {
            todo!();
        } else {
            ident = format!("{}%{}", var.ident, self.block_id);
            var_map = VariableMap {
                _ident: var.ident.clone(),
                offset: self.mem_offset,
                // TODO: Change size
                size,
                item_size,
                is_mut: var.mutable,
            };
        }
        self.mem_offset += size;
        if var.init_value.is_some() {
            // TODO: Type check
            let init_value = var.init_value.clone().unwrap();
            // this pushes result in stack
            self.compile_expr(&init_value);
            let mem_acss = format!(
                "{} [rbp-{}]",
                mem_word(var_map.item_size),
                var_map.offset + var_map.size
            );
            self.instruct_buf.push(asm!("pop rax"));
            self.instruct_buf
                .push(asm!("mov {mem_acss},{}", rbs("a", var_map.item_size)));
        }
        self.variables_map.insert(ident, var_map);
    }

    pub fn function_args(&mut self, args: &[FunctionArg]) {
        for (args_count, arg) in args.iter().enumerate() {
            let ident = format!("{}%{}", arg.ident, self.block_id);
            let map = VariableMap {
                _ident: arg.ident.clone(),
                offset: self.mem_offset,
                is_mut: false,
                item_size: 8,
                size: 8,
            };
            if args_count < 6 {
                let mem_acss = format!("{} [rbp-{}]", mem_word(8), map.offset + map.size);
                let reg = function_args_register(args_count, 8);
                self.instruct_buf.push(asm!("mov {},{}", mem_acss, reg));
            } else {
                todo!();
                // let mem_overload = format!("{} [rbp+{}]", mem_word(8), 16 + (args_count - 6) * 8);
                //let mem_acss = format!("{} [rbp-{}]", mem_word(8), map.offset + map.size);
                //self.instruct_buf
                //    .push(asm!("mov {},{}", mem_acss, mem_overload));
            }
            self.variables_map.insert(ident, map);
            self.mem_offset += 8;
        }
    }

    pub fn function(&mut self, f: Function) {
        self.scoped_blocks = Vec::new();
        self.block_id = 0;
        self.scoped_blocks.push(0);
        self.mem_offset = 0;
        self.variables_map = HashMap::new();
        if f.ident == "main" {
            self.instruct_buf.push("_start:\n".to_string());
        } else {
            self.instruct_buf.push(format!("{}:\n", f.ident));
        }

        // set rbp to stack pointer for this block
        let index_1 = self.instruct_buf.len();
        self.instruct_buf.push(String::new());
        let index_2 = self.instruct_buf.len();
        self.instruct_buf.push(String::new());
        let index_3 = self.instruct_buf.len();
        self.instruct_buf.push(String::new());

        self.function_args(&f.args);
        self.compile_block(&f.block);
        self.scoped_blocks.pop();
        // Call Exit Syscall
        if !self.variables_map.is_empty() {
            self.instruct_buf[index_1] = asm!("push rbp");
            self.instruct_buf[index_2] = asm!("mov rbp, rsp");
            self.instruct_buf[index_3] = asm!("sub rsp, {}", self.frame_size());
        }
        if f.ident == "main" {
            self.instruct_buf.push(asm!("mov rax, 60"));
            self.instruct_buf.push(asm!("mov rdi, 0"));
            self.instruct_buf.push(asm!("syscall"));
        } else {
            // revert rbp
            if !self.variables_map.is_empty() {
                //self.instruct_buf.push(asm!("pop rbp"));
                self.instruct_buf.push(asm!("leave"));
                self.instruct_buf.push(asm!("ret"));
            } else {
                self.instruct_buf.push(asm!("ret"));
            }
        }
    }

    pub fn compile_lib(
        &mut self,
        path: String,
        exports: Vec<String>,
    ) -> Result<(Vec<String>, Vec<String>), Box<dyn Error>> {
        let program = parse_file(path);
        for item in program.items {
            match item {
                ProgramItem::StaticVar(_s) => {
                    todo!();
                    // self.insert_variable(&s);
                }
                ProgramItem::Func(f) => {
                    if exports.contains(&f.ident) {
                        self.functions_map.insert(f.ident.clone(), f.clone());
                        self.function(f);
                    }
                }
                ProgramItem::Import(next_path, idents) => {
                    let mut new_path = String::new();
                    new_path.push_str(next_path.as_str());
                    new_path.push_str(".nmt");
                    self.compile_lib(new_path, idents)?;
                }
            }
        }
        Ok((self.instruct_buf.clone(), self.data_buf.clone()))
    }

    // TODO: Handle Compilation Error
    pub fn compile(&mut self, path: String) -> Result<(Vec<String>, Vec<String>), Box<dyn Error>> {
        let program = parse_file(path);
        for item in program.items {
            match item {
                ProgramItem::StaticVar(_s) => {
                    todo!();
                    // self.insert_variable(&s);
                }
                ProgramItem::Func(f) => {
                    self.functions_map.insert(f.ident.clone(), f.clone());
                    self.function(f);
                }
                ProgramItem::Import(next_path, idents) => {
                    let mut new_path = String::new();
                    new_path.push_str(next_path.as_str());
                    new_path.push_str(".nmt");
                    self.compile_lib(new_path, idents)?;
                }
            }
        }
        assert!(
            self.scoped_blocks.is_empty(),
            "Somting went wrong: Scope has not been cleared"
        );
        Ok((self.instruct_buf.clone(), self.data_buf.clone()))
    }

    /*
     *  keep in mind there could be a problem when a variable wants to access
     *  somthing that added after in code but it could be a feature too :)
     */
    fn compile_block(&mut self, block: &Block) {
        self.block_id += 1;
        self.scoped_blocks.push(self.block_id);
        for stmt in &block.stmts {
            self.compile_stmt(stmt);
        }
        self.block_id -= 1;
        self.scoped_blocks.pop().unwrap();
    }

    fn compile_if_stmt(&mut self, ifs: &IFStmt, exit_tag: usize) {
        self.compile_expr(&ifs.condition);
        let next_tag = match ifs.else_block.as_ref() {
            ElseBlock::None => exit_tag,
            _ => self.instruct_buf.len(),
        };
        self.instruct_buf.push(asm!("pop rax"));
        self.instruct_buf.push(asm!("test rax, rax"));
        self.instruct_buf.push(asm!("jz .L{}", next_tag));

        self.compile_block(&ifs.then_block);
        match ifs.else_block.as_ref() {
            ElseBlock::None => {
                self.instruct_buf.push(asm!(".L{}:", next_tag));
            }
            ElseBlock::Else(b) => {
                self.instruct_buf.push(asm!("jmp .L{}", exit_tag));
                self.instruct_buf.push(asm!(".L{}:", next_tag));
                self.compile_block(b);
                self.instruct_buf.push(asm!(".L{}:", exit_tag));
            }
            ElseBlock::Elif(iff) => {
                self.instruct_buf.push(asm!("jmp .L{}", exit_tag));
                self.instruct_buf.push(asm!(".L{}:", next_tag));
                self.compile_if_stmt(iff, exit_tag);
            }
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VariableDecl(v) => {
                self.insert_variable(v);
            }
            Stmt::Print(e) => {
                self.compile_expr(e);
                match e {
                    Expr::String(_) => {
                        self.instruct_buf.push(asm!("mov rax, 1"));
                        self.instruct_buf.push(asm!("mov rdi, 1"));
                        self.instruct_buf.push(asm!("pop rbx"));
                        self.instruct_buf.push(asm!("pop rcx"));
                        self.instruct_buf.push(asm!("mov rsi, rcx"));
                        self.instruct_buf.push(asm!("mov rdx, rbx"));
                        self.instruct_buf.push(asm!("syscall"));
                    }
                    _ => {
                        self.instruct_buf.push(asm!("pop rdi"));
                        self.instruct_buf.push(asm!("call print"));
                    }
                }
            }
            Stmt::If(ifs) => {
                let exit_tag = self.instruct_buf.len();
                self.compile_if_stmt(ifs, exit_tag);
            }
            Stmt::Assgin(a) => {
                self.compile_assgin(a);
            }
            Stmt::While(w) => {
                self.compile_while(w);
            }
            Stmt::Expr(e) => match e {
                Expr::FunctionCall(_) => {
                    self.compile_expr(e);
                }
                _ => {
                    println!("Warning: Expretion with no effect ignored!");
                }
            },
            Stmt::Return(e) => {
                self.compile_expr(e);
                self.instruct_buf.push(asm!("pop rax"));
                self.instruct_buf.push(asm!("leave"));
                self.instruct_buf.push(asm!("ret"));
                println!("Warning: might segfault add leave or fix dataframe");
            }
            Stmt::InlineAsm(instructs) => {
                for instr in instructs {
                    self.compile_inline_asm(instr);
                }
            }
            _ => {
                todo!();
            }
        }
    }

    fn compile_inline_asm(&mut self, instr: &String) {
        if instr.contains('%') {
            let mut final_instr = instr.clone();
            let chars = final_instr.chars().collect::<Vec<char>>();
            let mut index = 0;
            let is_empty = |index: usize| (index >= chars.len());
            while !is_empty(index) {
                if chars[index] == '%' {
                    let mut ident = String::new();
                    let first_index = index;
                    index += 1;
                    while !is_empty(index)
                        && (chars[index].is_alphanumeric() || chars[index] == '_')
                    {
                        ident.push(chars[index]);
                        index += 1;
                    }
                    if !ident.is_empty() {
                        let v_map = self.find_variable(ident.clone()).unwrap_or_else(|| {
                            eprintln!(
                                "Error: Could not find variable {} in this scope",
                                ident.clone()
                            );
                            exit(1);
                        });
                        let mem_acss =
                            format!("{} [rbp-{}]", mem_word(8), v_map.offset + v_map.size);
                        let mut temp = String::new();
                        temp.push_str(chars[0..(first_index)].iter().collect::<String>().as_str());
                        temp.push_str(mem_acss.as_str());
                        temp.push_str(chars[index..].iter().collect::<String>().as_str());
                        final_instr = temp;
                        index += mem_acss.len()
                    } else {
                        eprintln!("Error: Invalid Identifier for Inline Asm");
                        exit(1);
                    }
                } else {
                    index += 1;
                }
            }
            self.instruct_buf.push(asm!("{}", final_instr));
        } else {
            self.instruct_buf.push(asm!("{}", instr));
        }
    }

    fn compile_while(&mut self, w_stmt: &WhileStmt) {
        let cond_tag = self.instruct_buf.len();
        self.instruct_buf.push(asm!("jmp .L{}", cond_tag));
        let block_tag = cond_tag + 1;
        self.instruct_buf.push(asm!(".L{}:", block_tag));
        self.compile_block(&w_stmt.block);
        self.instruct_buf.push(asm!(".L{}:", cond_tag));
        // Jump after a compare
        self.compile_expr(&w_stmt.condition);
        self.instruct_buf.push(asm!("pop rax"));
        self.instruct_buf.push(asm!("test rax, rax"));
        self.instruct_buf.push(asm!("jnz .L{}", block_tag));
    }

    fn assgin_op(&mut self, op: &AssginOp, v_map: &VariableMap) {
        let mem_acss = if v_map.item_size != v_map.size {
            format!(
                "{} [rbp-{}+rbx*{}]",
                mem_word(v_map.item_size),
                v_map.offset + v_map.size,
                v_map.item_size
            )
        } else {
            format!(
                "{} [rbp-{}]",
                mem_word(v_map.item_size),
                v_map.offset + v_map.size
            )
        };
        let reg = rbs("a", v_map.item_size);
        self.instruct_buf.push(asm!("pop rax"));
        match op {
            AssginOp::Eq => {
                self.instruct_buf.push(asm!("mov {mem_acss},{reg}"));
            }
            AssginOp::PlusEq => {
                self.instruct_buf.push(asm!("add {mem_acss},{reg}"));
            }
            AssginOp::SubEq => {
                self.instruct_buf.push(asm!("sub {mem_acss},{reg}"));
            }
            AssginOp::MultiEq => {
                self.instruct_buf.push(asm!("imul {mem_acss},{reg}"));
            }
            AssginOp::DevideEq => {
                // self.instruct_buf.push(asm!("cdq"));
                self.instruct_buf.push(asm!("idiv rbx"));
                self.instruct_buf.push(asm!("mov {mem_acss},{reg}"));
            }
            AssginOp::ModEq => {
                self.instruct_buf.push(asm!("cdq"));
                self.instruct_buf.push(asm!("idiv rbx"));
                self.instruct_buf.push(asm!("mov {mem_acss},{reg}"));
            }
        }
    }

    fn compile_assgin(&mut self, assign: &Assgin) {
        match &assign.left {
            Expr::Variable(v) => {
                let v_map = self.get_vriable_map(v);
                if !v_map.is_mut {
                    eprintln!("Error: Variable is not mutable. Did you forgot to define it with '=' insted of ':=' ?");
                    exit(1);
                }
                self.compile_expr(&assign.right);
                self.assgin_op(&assign.op, &v_map);
            }
            Expr::ArrayIndex(ai) => {
                let v_map = self.get_vriable_map(&ai.ident);
                if !v_map.is_mut {
                    eprintln!("Error: Variable is not mutable. Did you forgot to define it with '=' insted of ':=' ?");
                    exit(1);
                }
                self.compile_expr(&assign.right);
                self.compile_expr(&ai.indexer);
                self.instruct_buf.push(asm!("pop rbx"));
                self.assgin_op(&assign.op, &v_map);
            }
            _ => {
                eprintln!("Error: Expected a Variable type expression found Value");
                exit(1);
            }
        }
    }

    fn get_vriable_map(&mut self, var_ident: &String) -> VariableMap {
        self.find_variable(var_ident.clone()).unwrap_or_else(|| {
            eprintln!("Error: Trying to access an Undifined variable ({var_ident})");
            exit(1);
        })
    }

    fn compile_expr(&mut self, expr: &Expr) {
        // left = compile expr
        // right = compile expr
        // +
        match expr {
            Expr::Variable(v) => {
                let v_map = self.get_vriable_map(v);
                let mem_acss = format!(
                    "{} [rbp-{}]",
                    mem_word(v_map.item_size),
                    v_map.offset + v_map.size
                );
                self.instruct_buf
                    .push(asm!("mov {},{mem_acss}", rbs("a", v_map.item_size)));
                self.instruct_buf.push(asm!("push rax"));
            }
            Expr::Char(x) => {
                self.instruct_buf.push(asm!("push {x}"));
            }
            Expr::Int(x) => {
                // push x
                self.instruct_buf.push(asm!("push {x}"));
            }
            Expr::Compare(c) => {
                // TODO: Convert exprs to 0 or 1 and push into stack
                self.compile_expr(c.left.as_ref());
                self.compile_expr(c.right.as_ref());
                self.instruct_buf.push(asm!("mov rcx, 0"));
                self.instruct_buf.push(asm!("mov rdx, 1"));
                self.instruct_buf.push(asm!("pop rbx"));
                self.instruct_buf.push(asm!("pop rax"));
                self.instruct_buf.push(asm!("cmp rax, rbx"));
                match c.op {
                    CompareOp::Eq => {
                        self.instruct_buf.push(asm!("cmove rcx, rdx"));
                    }
                    CompareOp::NotEq => {
                        self.instruct_buf.push(asm!("cmovne rcx, rdx"));
                    }
                    CompareOp::Bigger => {
                        self.instruct_buf.push(asm!("cmovg rcx, rdx"));
                    }
                    CompareOp::Smaller => {
                        self.instruct_buf.push(asm!("cmovl rcx, rdx"));
                    }
                    CompareOp::BiggerEq => {
                        self.instruct_buf.push(asm!("cmovge rcx, rdx"));
                    }
                    CompareOp::SmallerEq => {
                        self.instruct_buf.push(asm!("cmovle rcx, rdx"));
                    }
                }
                self.instruct_buf.push(asm!("push rcx"));
            }
            Expr::Binary(b) => {
                self.compile_expr(b.left.as_ref());
                self.compile_expr(b.right.as_ref());
                self.instruct_buf.push(asm!("pop rbx"));
                self.instruct_buf.push(asm!("pop rax"));
                match b.op {
                    Op::Plus => {
                        self.instruct_buf.push(asm!("add rax, rbx"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Sub => {
                        self.instruct_buf.push(asm!("sub rax, rbx"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Multi => {
                        self.instruct_buf.push(asm!("imul rax, rbx"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Devide => {
                        self.instruct_buf.push(asm!("cqo"));
                        self.instruct_buf.push(asm!("idiv rbx"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Mod => {
                        self.instruct_buf.push(asm!("cqo"));
                        self.instruct_buf.push(asm!("idiv rbx"));
                        self.instruct_buf.push(asm!("push rdx"));
                    }
                    Op::Or => {
                        self.instruct_buf.push(asm!("or rax, rbx"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::And => {
                        self.instruct_buf.push(asm!("and rax, rbx"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Lsh => {
                        self.instruct_buf.push(asm!("mov rcx, rbx"));
                        self.instruct_buf.push(asm!("sal rax, cl"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Rsh => {
                        self.instruct_buf.push(asm!("mov rcx, rbx"));
                        self.instruct_buf.push(asm!("sar rax, cl"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Not => {
                        panic!("Unvalid binary operation");
                    }
                }
            }
            Expr::String(str) => {
                let id = self.data_buf.len();
                let data_array = Self::asmfy_string(str);
                self.data_buf.push(asm!("data{id} db {}", data_array));
                self.data_buf.push(asm!("len{id} equ $ - data{id}"));
                self.instruct_buf.push(asm!("push data{id}"));
                self.instruct_buf.push(asm!("push len{id}"));
                // data6524 db "<str>"
                // len6524     data6524
                // push len6524jkjk
                // push data6524
                // self.instruct_buf.push(asm!("push 13"));
            }
            Expr::Unary(u) => {
                self.compile_unary(u);
                self.instruct_buf.push(asm!("pop rax"));
                match u.op {
                    Op::Sub => {
                        self.instruct_buf.push(asm!("neg rax"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Plus => {
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    Op::Not => {
                        self.instruct_buf.push(asm!("not rax"));
                        self.instruct_buf.push(asm!("push rax"));
                    }
                    _ => {
                        unreachable!();
                    }
                }
            }
            Expr::FunctionCall(fc) => {
                self.compile_function_call(fc);
            }
            Expr::Ptr(e) => {
                self.compile_ptr(e);
            }
            Expr::ArrayIndex(ai) => {
                let v_map = self.find_variable(ai.ident.clone()).unwrap_or_else(|| {
                    eprintln!(
                        "Error: Trying to access an Undifined variable ({})",
                        ai.ident
                    );
                    exit(1);
                });
                self.compile_expr(&ai.indexer);
                self.instruct_buf.push(asm!("pop rbx"));
                // TODO: Add Item size to v_map
                let mem_acss = format!(
                    "{} [rbp-{}+rbx*{}]",
                    mem_word(v_map.item_size),
                    v_map.offset + v_map.size,
                    v_map.item_size
                );
                let reg = rbs("a", v_map.item_size);
                self.instruct_buf.push(asm!("mov {reg},{mem_acss}"));
                self.instruct_buf.push(asm!("push {reg}"));
            }
        }
    }

    fn compile_unary(&mut self, unary: &UnaryExpr) {
        self.compile_expr(&unary.right);
    }

    fn compile_ptr(&mut self, expr: &Expr) {
        match expr {
            Expr::Variable(v) => {
                let v_map = self.get_vriable_map(v);
                self.instruct_buf.push(asm!("mov rax, rbp"));
                self.instruct_buf
                    .push(asm!("sub rax, {}", v_map.offset + v_map.size));
                self.instruct_buf.push(asm!("push rax"));
            }
            _ => {
                todo!("Impl Pointers");
            }
        }
    }

    fn compile_function_call(&mut self, fc: &FunctionCall) {
        for (index, arg) in fc.args.iter().enumerate() {
            self.compile_expr(arg);
            match arg {
                Expr::String(_) => {
                    self.instruct_buf.push(asm!("pop rax"));
                    self.instruct_buf
                        .push(asm!("pop {}", function_args_register(index, 8)));
                }
                _ => {
                    self.instruct_buf
                        .push(asm!("pop {}", function_args_register(index, 8)));
                }
            }
        }
        // TODO: Setup a unresolved function table
        let fun = self.functions_map.get(&fc.ident).unwrap_or_else(|| {
            eprintln!("Error: Function {} is not avaliable in this scope.",&fc.ident);
            eprintln!("Make sure you are calling the correct function");
            exit(-1);
        });
        self.instruct_buf.push(asm!("mov rax, 0"));
        self.instruct_buf.push(asm!("call {}", fc.ident));
        if fun.ret_type.is_some() {
            self.instruct_buf.push(asm!("push rax"));
        }
    }

    fn asmfy_string(str: &str) -> String {
        let mut res = String::new();
        let source: Vec<char> = str.chars().collect();
        let mut i = 0;
        while i < source.len() {
            match source[i] {
                '\n' => {
                    if !res.is_empty() {
                        res.push(',');
                    }
                    res.push_str(" 10");
                    i += 1;
                }
                '\t' => {
                    if !res.is_empty() {
                        res.push(',');
                    }
                    res.push_str(" 9");
                    i += 1;
                }
                '\r' => {
                    if !res.is_empty() {
                        res.push(',');
                    }
                    res.push_str(" 13");
                    i += 1;
                }
                '\"' => {
                    if !res.is_empty() {
                        res.push(',');
                    }
                    res.push_str(" 34");
                    i += 1;
                }
                _ => {
                    if !res.is_empty() {
                        res.push(',');
                    }
                    res.push('\"');
                    while i < source.len() {
                        if source[i] == '\n'
                            || source[i] == '\"'
                            || source[i] == '\t'
                            || source[i] == '\r'
                        {
                            break;
                        }
                        res.push(source[i]);
                        i += 1;
                    }
                    res.push('\"');
                }
            }
        }
        res
    }
}
