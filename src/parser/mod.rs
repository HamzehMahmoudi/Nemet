pub mod block;
pub mod expr;
pub mod function;
pub mod program;
pub mod stmt;
use std::fs;
use std::process::exit;

use crate::lexer::{Lexer, TokenType};

use crate::parser::block::*;
use crate::parser::expr::{
    ArrayIndex, BinaryExpr, CompareExpr, CompareOp, Expr, FunctionCall, Op, UnaryExpr,
};
use crate::parser::function::*;
use crate::parser::program::*;
use crate::parser::stmt::*;

pub fn parse_file(path: String) -> ProgramFile {
    let source = fs::read_to_string(path.clone()).expect("Can not Read the file");
    let mut lexer = Lexer::new(path, source);
    program(&mut lexer)
}

// -4 -> 4 neg
// 4 + 2 -> 4 2 +
// 4 * 3 + 6 -> 4 3 * 6 +
// 4 + (3 + 6) -> 3 6 + 4 +
// -(4 * cos(0) + 2 - 6) -> 4 cos(0) * 2 + 6 - neg
pub fn expr(lexer: &mut Lexer) -> Expr {
    let mut term_expr = term(lexer);
    loop {
        let t_type = lexer.get_token_type();
        if Expr::is_binary_op(t_type) {
            let op = Op::from_token_type(t_type);
            lexer.next_token();
            let right = term(lexer);
            term_expr = Expr::Binary(BinaryExpr {
                left: Box::new(term_expr),
                op,
                right: Box::new(right),
            });
        } else if Expr::is_compare_op(t_type) {
            let op = CompareOp::from_token_type(lexer.get_token_type());
            lexer.next_token();
            let right = term(lexer);
            term_expr = Expr::Compare(CompareExpr {
                left: Box::new(term_expr),
                op,
                right: Box::new(right),
            });
        } else {
            break;
        }
    }
    term_expr
}

pub fn term(lexer: &mut Lexer) -> Expr {
    let mut left = factor(lexer);
    while lexer.get_token_type() == TokenType::Multi
        || lexer.get_token_type() == TokenType::Devide
        || lexer.get_token_type() == TokenType::Mod
        || lexer.get_token_type() == TokenType::Lsh
        || lexer.get_token_type() == TokenType::Rsh
    {
        let op = Op::from_token_type(lexer.get_token_type());
        lexer.next_token();
        let right = factor(lexer);
        left = Expr::Binary(BinaryExpr {
            left: Box::new(left),
            op,
            right: Box::new(right),
        });
    }
    left
}

pub fn factor(lexer: &mut Lexer) -> Expr {
    match lexer.get_token_type() {
        TokenType::OParen => {
            lexer.match_token(TokenType::OParen);
            let value = expr(lexer);
            lexer.match_token(TokenType::CParen);
            value
        }
        TokenType::Plus | TokenType::Minus | TokenType::Not => {
            let op = Op::from_token_type(lexer.get_token_type());
            lexer.next_token();
            let value = factor(lexer);
            Expr::Unary(UnaryExpr {
                op,
                right: Box::new(value),
            })
        }
        TokenType::String => {
            let str_token = lexer.get_token();
            lexer.next_token();
            Expr::String(str_token.literal)
        }
        TokenType::Ptr => {
            lexer.match_token(TokenType::Ptr);
            let value = expr(lexer);
            Expr::Ptr(Box::new(value))
        }
        TokenType::True => {
            lexer.match_token(TokenType::True);
            Expr::Int(1)
        }
        TokenType::False => {
            lexer.match_token(TokenType::False);
            Expr::Int(0)
        }
        TokenType::Char(c) => {
            lexer.next_token();
            Expr::Char(c as u8)
        }
        TokenType::Int(val) => {
            lexer.next_token();
            Expr::Int(val)
        }
        TokenType::Identifier => {
            let ident_name = lexer.get_token().literal;
            if lexer.next_token().is_empty() {
                return Expr::Variable(ident_name);
            }
            match lexer.get_token_type() {
                TokenType::OParen => {
                    let args = function_call_args(lexer);
                    Expr::FunctionCall(FunctionCall {
                        ident: ident_name,
                        args,
                    })
                }
                TokenType::OBracket => {
                    let indexer = array_indexer(lexer);
                    Expr::ArrayIndex(ArrayIndex {
                        ident: ident_name,
                        indexer: Box::new(indexer),
                    })
                }
                _ => Expr::Variable(ident_name),
            }
        }
        _ => {
            eprintln!(
                "Unexpected Token ({:?}) while parsing expr at {}",
                lexer.get_token_type(),
                lexer.get_loc_string()
            );
            exit(-1);
        }
    }
}

pub fn array_indexer(lexer: &mut Lexer) -> Expr {
    lexer.match_token(TokenType::OBracket);
    let index = expr(lexer);
    lexer.match_token(TokenType::CBracket);
    index
}

pub fn function_call_args(lexer: &mut Lexer) -> Vec<Expr> {
    let mut args = Vec::<Expr>::new();
    lexer.match_token(TokenType::OParen);
    loop {
        //|| | expr | expr , expr
        match lexer.get_token_type() {
            TokenType::CParen => {
                lexer.match_token(TokenType::CParen);
                break;
            }
            _ => {
                args.push(expr(lexer));
                if lexer.get_token_type() == TokenType::Comma {
                    lexer.match_token(TokenType::Comma);
                }
            }
        }
    }
    args
}

pub fn function_def(lexer: &mut Lexer) -> Function {
    lexer.match_token(TokenType::Func);
    let function_ident_token = lexer.get_token();
    let mut ret_type: Option<VariableType> = None;
    if function_ident_token.is_empty() {
        eprintln!(
            "Function Defenition without Identifier at {}",
            lexer.get_loc_string()
        );
        exit(-1);
    }
    lexer.match_token(TokenType::Identifier);
    let args = function_def_args(lexer);
    if lexer.get_token_type() == TokenType::ATSign {
        ret_type = Some(type_def(lexer));
    }
    let block = block(lexer);
    Function {
        ident: function_ident_token.literal,
        ret_type,
        args,
        block,
    }
}

/*
 * Stmt := {declare | expr { = expr}} ;
 * declare := let Ident = expr;
*/

pub fn if_stmt(lexer: &mut Lexer) -> IFStmt {
    lexer.match_token(TokenType::If);
    let condition = expr(lexer);
    let then_block = block(lexer);
    if lexer.get_token_type() == TokenType::Else {
        lexer.match_token(TokenType::Else);
        if lexer.get_token_type() == TokenType::If {
            let else_block = Box::new(ElseBlock::Elif(if_stmt(lexer)));
            IFStmt {
                condition,
                then_block,
                else_block,
            }
        } else {
            let else_block = Box::new(ElseBlock::Else(block(lexer)));
            IFStmt {
                condition,
                then_block,
                else_block,
            }
        }
    } else {
        IFStmt {
            condition,
            then_block,
            else_block: Box::new(ElseBlock::None),
        }
    }
}

pub fn while_stmt(lexer: &mut Lexer) -> WhileStmt {
    lexer.match_token(TokenType::While);
    let condition = expr(lexer);
    let block = block(lexer);
    WhileStmt { condition, block }
}

pub fn block(lexer: &mut Lexer) -> Block {
    lexer.match_token(TokenType::OCurly);
    let mut stmts = Vec::<Stmt>::new();
    loop {
        if lexer.get_token_type() == TokenType::CCurly {
            break;
        }
        match lexer.get_token_type() {
            TokenType::Var => {
                stmts.push(Stmt::VariableDecl(variable_declare(lexer)));
                lexer.match_token(TokenType::SemiColon);
            }
            TokenType::Print => {
                lexer.match_token(TokenType::Print);
                let expr = expr(lexer);
                stmts.push(Stmt::Print(expr));
                lexer.match_token(TokenType::SemiColon);
            }
            TokenType::Break => {
                lexer.match_token(TokenType::Break);
                stmts.push(Stmt::Break);
                lexer.match_token(TokenType::SemiColon);
            }
            TokenType::Continue => {
                lexer.match_token(TokenType::Continue);
                stmts.push(Stmt::Continue);
                lexer.match_token(TokenType::SemiColon);
            }
            TokenType::If => {
                stmts.push(Stmt::If(if_stmt(lexer)));
            }
            TokenType::While => {
                stmts.push(Stmt::While(while_stmt(lexer)));
            }
            TokenType::Return => {
                lexer.match_token(TokenType::Return);
                stmts.push(Stmt::Return(expr(lexer)));
                lexer.match_token(TokenType::SemiColon);
            }
            TokenType::Identifier => {
                //Assgin Op
                let left_expr = expr(lexer);
                let token_type = lexer.get_token_type();
                if token_type == TokenType::SemiColon {
                    stmts.push(Stmt::Expr(left_expr));
                } else if token_type.is_assgin_token() {
                    let op_type = AssginOp::from_token_type(&token_type);
                    lexer.match_token(token_type);
                    let right_expr = expr(lexer);
                    stmts.push(Stmt::Assgin(Assgin {
                        left: left_expr,
                        right: right_expr,
                        op: op_type,
                    }));
                } else {
                    eprintln!("Error: Expected Semicolon at {}", lexer.get_loc_string());
                    exit(-1);
                }
                lexer.match_token(TokenType::SemiColon);
            }
            TokenType::Asm => {
                lexer.match_token(TokenType::Asm);
                lexer.match_token(TokenType::OCurly);
                let mut instructs = Vec::<String>::new();
                while lexer.get_token_type() == TokenType::String {
                    instructs.push(lexer.get_token().literal);
                    lexer.match_token(TokenType::String);
                }
                lexer.match_token(TokenType::CCurly);
                stmts.push(Stmt::InlineAsm(instructs));
            }
            _ => {
                todo!();
            }
        }
    }
    lexer.match_token(TokenType::CCurly);
    Block { stmts }
}

pub fn type_def(lexer: &mut Lexer) -> VariableType {
    lexer.match_token(TokenType::ATSign);
    match lexer.get_token_type() {
        TokenType::Ptr => {
            lexer.match_token(TokenType::Ptr);
            VariableType::Pointer
        }
        TokenType::Identifier => {
            let ident = lexer.get_token().literal;
            lexer.match_token(TokenType::Identifier);
            VariableType::from_string(ident)
        }
        TokenType::OBracket => {
            let var_type: VariableType;
            let size: usize;
            lexer.match_token(TokenType::OBracket);
            let token = lexer.get_token();
            if token.is_empty() {
                eprintln!(
                    "Error: Expected an Identifier found EOF at {}",
                    lexer.get_loc_string()
                );
                exit(1);
            }
            if token.t_type == TokenType::Identifier {
                var_type = VariableType::from_string(lexer.get_token().literal);
                lexer.match_token(TokenType::Identifier);
            } else if token.t_type == TokenType::ATSign {
                var_type = self::type_def(lexer);
            } else {
                eprintln!(
                    "Error: Expected Identifier found {:?}, at {}",
                    lexer.get_token_type(),
                    lexer.get_loc_string()
                );
                exit(1);
            }
            lexer.match_token(TokenType::Comma);
            let token = lexer.get_token();
            if token.is_empty() {
                eprintln!(
                    "Error: Expected a Number found EOF at {}",
                    lexer.get_loc_string()
                );
                exit(1);
            }
            match token.t_type {
                TokenType::Int(s) => {
                    size = s as usize;
                    lexer.match_token(TokenType::Int(s));
                }
                TokenType::QMark => {
                    lexer.match_token(TokenType::QMark);
                    return VariableType::Pointer;
                }
                _ => {
                    eprintln!(
                        "Error: Expected Integer Number found {:?}, at {}",
                        lexer.get_token_type(),
                        lexer.get_loc_string()
                    );
                    exit(1);
                }
            }
            lexer.match_token(TokenType::CBracket);
            VariableType::Array(Box::new(var_type), size)
        }
        _ => {
            eprintln!("Syntax Error: Unknown Token at {}", lexer.get_loc_string());
            exit(1);
        }
    }
}

pub fn variable_declare(lexer: &mut Lexer) -> VariableDeclare {
    lexer.match_token(TokenType::Var);
    let ident_token = lexer.get_token();
    lexer.match_token(TokenType::Identifier);
    let mut is_mutable: bool = true;
    let mut is_static: bool = false;
    let mut v_type: Option<VariableType> = None;
    let mut init_value: Option<Expr> = None;
    if lexer.get_token_type() == TokenType::ATSign {
        v_type = Some(type_def(lexer));
    }
    match lexer.get_token_type() {
        TokenType::DoubleColon => {
            is_static = true;
            is_mutable = false;
            lexer.match_token(TokenType::ColonEq);
            init_value = Some(expr(lexer));
        }
        TokenType::ColonEq => {
            is_mutable = false;
            lexer.match_token(TokenType::ColonEq);
            init_value = Some(expr(lexer));
        }
        TokenType::Eq => {
            is_mutable = true;
            lexer.match_token(TokenType::Eq);
            init_value = Some(expr(lexer));
        }
        TokenType::SemiColon => {}
        _ => {
            eprintln!(
                "Error: Expected \"=\" or \":=\" found ({:?}) at {}",
                lexer.get_token_type(),
                lexer.get_loc_string()
            );
            exit(-1);
        }
    }
    VariableDeclare {
        mutable: is_mutable,
        is_static,
        ident: ident_token.literal,
        v_type,
        init_value,
    }
}

pub fn function_def_args(lexer: &mut Lexer) -> Vec<FunctionArg> {
    let mut args = Vec::<FunctionArg>::new();
    lexer.match_token(TokenType::OParen);
    loop {
        match lexer.get_token_type() {
            TokenType::CParen => {
                lexer.match_token(TokenType::CParen);
                break;
            }
            TokenType::Identifier => {
                let ident = lexer.get_token().literal;
                lexer.match_token(TokenType::Identifier);
                let typedef = type_def(lexer);
                if lexer.get_token_type() == TokenType::Comma {
                    lexer.match_token(TokenType::Comma);
                }
                args.push(FunctionArg {
                    ident: ident.to_string(),
                    typedef,
                });
            }
            _ => {
                eprintln!(
                    "Error: Expected Identifier found ({:?}) at {}",
                    lexer.get_token_type(),
                    lexer.get_loc_string()
                );
                exit(-1);
            }
        }
    }
    args
}

pub fn program(lexer: &mut Lexer) -> ProgramFile {
    lexer.next_token();
    let mut items = Vec::<ProgramItem>::new();
    loop {
        if lexer.get_token().is_empty() {
            break;
        }
        match lexer.get_token_type() {
            TokenType::Func => {
                items.push(ProgramItem::Func(function_def(lexer)));
            }
            TokenType::Var => {
                items.push(ProgramItem::StaticVar(variable_declare(lexer)));
            }
            TokenType::Import => items.push(import_file(lexer)),
            _ => {
                eprintln!(
                    "Error: Unexpected Token ({:?}) for top level program at {}",
                    lexer.get_token_type(),
                    lexer.get_loc_string()
                );
                exit(-1);
            }
        }
    }
    ProgramFile {
        shebang: String::new(),
        file_path: lexer.file_path.clone(),
        items,
    }
}

pub fn import_file(lexer: &mut Lexer) -> ProgramItem {
    lexer.match_token(TokenType::Import);
    let file_path = lexer.get_token().literal;
    lexer.match_token(TokenType::String);
    if lexer.get_token_type() == TokenType::DoubleColon {
        lexer.match_token(TokenType::DoubleColon);
        let mut idents_vec = Vec::<String>::new();
        loop {
            let ident = lexer.get_token().literal;
            lexer.match_token(TokenType::Identifier);
            idents_vec.push(ident);
            if lexer.get_token_type() == TokenType::Comma {
                lexer.match_token(TokenType::Comma);
            } else {
                break;
            }
        }
        ProgramItem::Import(file_path, idents_vec)
    } else {
        ProgramItem::Import(file_path, vec![])
    }
}
