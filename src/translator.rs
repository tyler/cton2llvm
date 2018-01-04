use std::str;
use std::collections::hash_map::{HashMap, Entry};
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::values::{FunctionValue, PhiValue, IntValue};
use inkwell::types;
use inkwell::basic_block::BasicBlock;
use inkwell::IntPredicate;
use cretonne::ir;
use cretonne::ir::immediates::Imm64;
use cretonne::cursor::{FuncCursor, Cursor, CursorPosition};
use parser::CtonModule;


pub fn translate(cton_mod: CtonModule) -> Result<Module, String> {
    let mut translator = Translator::new();
    translator.translate(cton_mod).expect("translated module");
    Ok(translator.module)
}

pub struct Translator {
    context: Context,
    module: Module,
    builder: Builder,
    data: TranslatorData,
}

struct TranslatorData {
    ebb_to_entry: HashMap<ir::Ebb, BasicBlock>,
}

struct FunctionContext<'a> {
    func: FunctionValue,
    pos: FuncCursor<'a>,
    values: HashMap<ir::Value, TV>,
}

impl<'a> FunctionContext<'a> {
    pub fn insert_value(&mut self, key: ir::Value, value: TV) {
        self.values.insert(key, value);
    }

    pub fn entity_pool(&self) -> &ir::ValueListPool {
        &self.pos.func.dfg.value_lists
    }
}

impl Translator {
    pub fn new() -> Self {
        let context = Context::create();
        let module = context.create_module("cton2llvm");
        let builder = context.create_builder();

        Self {
            context: context,
            module: module,
            builder: builder,
            data: TranslatorData { ebb_to_entry: HashMap::new() },
        }
    }

    pub fn translate(&mut self, cton_mod: CtonModule) -> Result<(), String> {
        for cton_func in &cton_mod.functions {
            self.translate_function(&mut cton_func.clone());
        }

        Ok(())
    }

    fn translate_function(&mut self, cton_func: &mut ir::Function) {
        let func = self.create_func(cton_func);
        let pos = FuncCursor::new(cton_func);

        let mut fnctx = FunctionContext {
            func: func,
            pos: pos,
            values: HashMap::new(),
        };

        // Set up all the Ebb entry blocks
        while let Some(ebb) = fnctx.pos.next_ebb() {
            //println!("Setting up ebb: {:?}", ebb);
            self.translate_ebb_params(ebb, &mut fnctx);
        }

        // Reset the cursor
        let entry_ebb = fnctx.pos.func.layout.entry_block().unwrap();
        fnctx.pos.set_position(CursorPosition::Nowhere);

        // Translate the Ebbs
        while let Some(ebb) = fnctx.pos.next_ebb() {
            //println!("Translating ebb: {:?}", ebb);
            self.translate_ebb(ebb, &mut fnctx);
        }

        // Hook up the incoming args to the entry block phis
        let entry_bb = fnctx.func.get_entry_basic_block().unwrap();
        let new_entry_bb = entry_bb.prepend_basic_block("entry");
        self.builder.position_at_end(&new_entry_bb);
        self.builder.build_unconditional_branch(&entry_bb);

        for (i, param) in fnctx.pos.func.dfg.ebb_params(entry_ebb).iter().enumerate() {
            let phi = fnctx.values[param].into_phi_value();
            //println!("i={:?} param={:?} phi={:?}", i, param, phi);
            let fn_arg = fnctx.func.get_nth_param(i as u32).unwrap();
            phi.add_incoming(&[(&fn_arg, &new_entry_bb)]);
        }
    }

    fn translate_ebb_params(&mut self, ebb: ir::Ebb, fnctx: &mut FunctionContext) {
        {
            // Set the position of the Builder, then let the BasicBlock fall
            // out of scope, so we can use the `data` struct again.
            let bb = self.data.entry_bb_for_ebb(ebb, fnctx.func);
            self.builder.position_at_end(&bb);
        }

        let i32_ty = self.context.i64_type();
        let mut phis_params = Vec::new();
        for param in fnctx.pos.func.dfg.ebb_params(ebb).clone() {
            let phi = self.builder.build_phi(&i32_ty, "");
            phis_params.push((*param, phi));
        }
        for (param, phi) in phis_params {
            fnctx.insert_value(param, TV::phi(phi));
        }
    }

    fn translate_ebb(&mut self, ebb: ir::Ebb, fnctx: &mut FunctionContext) {
        {
            // Set the position of the Builder, then let the BasicBlock fall
            // out of scope, so we can use the `data` struct again.
            let bb = self.data.entry_bb_for_ebb(ebb, fnctx.func);
            self.builder.position_at_end(&bb);
        }

        while let Some(inst) = fnctx.pos.next_inst() {
            //println!("inst: {:?}", fnctx.pos.func.dfg[inst]);
            self.translate_instruction(inst, fnctx);
            //println!("values: {:?}", fnctx.values);
            //self.module.print_to_stderr();
        }
    }

    fn translate_instruction(&mut self, inst: ir::Inst, fnctx: &mut FunctionContext) {
        let inst_data = fnctx.pos.func.dfg[inst].clone();
        match inst_data {
            ir::InstructionData::Binary { opcode, args } => {
                self.translate_binary(inst, opcode, args, fnctx)
            }
            ir::InstructionData::Branch {
                opcode,
                destination,
                ref args,
            } => self.translate_branch(inst, opcode, destination, args, fnctx),
            ir::InstructionData::Jump {
                opcode,
                destination,
                ref args,
            } => self.translate_jump(inst, opcode, destination, args, fnctx),
            ir::InstructionData::UnaryImm { opcode, imm } => {
                self.translate_unary_imm(inst, opcode, imm, fnctx)
            }
            ir::InstructionData::MultiAry { opcode, ref args } => {
                self.translate_multiary(inst, opcode, args, fnctx)
            }
            _ => println!("unknown inst: {:?}", inst),
        }
    }

    fn translate_binary(
        &mut self,
        inst: ir::Inst,
        opcode: ir::Opcode,
        args: [ir::Value; 2],
        fnctx: &mut FunctionContext,
    ) {
        match opcode {
            ir::Opcode::Iadd => {
                let lhs = fnctx.values[&args[0]].into_int_value();
                let rhs = fnctx.values[&args[1]].into_int_value();
                let result = self.builder.build_int_add(&lhs, &rhs, "");
                let cton_result = fnctx.pos.func.dfg.inst_results(inst)[0];
                fnctx.insert_value(cton_result, TV::int(result));
            }
            ir::Opcode::Isub => {
                let lhs = fnctx.values[&args[0]].into_int_value();
                let rhs = fnctx.values[&args[1]].into_int_value();
                let result = self.builder.build_int_sub(&lhs, &rhs, "");
                let cton_result = fnctx.pos.func.dfg.inst_results(inst)[0];
                fnctx.insert_value(cton_result, TV::int(result));
            }
            _ => panic!("Unknown op: {:?}", opcode),
        }
    }

    fn translate_branch(
        &mut self,
        _inst: ir::Inst,
        opcode: ir::Opcode,
        dst: ir::Ebb,
        args: &ir::ValueList,
        fnctx: &mut FunctionContext,
    ) {
        match opcode {
            ir::Opcode::Brz => {
                let dst_bb = self.data.entry_bb_for_ebb(dst, fnctx.func);
                let new_bb = fnctx.func.append_basic_block("");
                let curr_bb = self.builder.get_insert_block().unwrap();

                // Get the args
                let args = args.as_slice(fnctx.entity_pool());

                // Set up the condition
                let cmp_arg = args[0];
                let cmp_arg_ty = self.context.i64_type();
                let lhs = &fnctx.values[&cmp_arg].into_int_value();
                let rhs = cmp_arg_ty.const_int(0, false);

                let cmp_val = self.builder.build_int_compare(
                    IntPredicate::EQ,
                    &lhs,
                    &rhs,
                    "",
                );

                self.builder.build_conditional_branch(
                    &cmp_val,
                    &dst_bb,
                    &new_bb,
                );

                let inst_dst_args = &args[1..].iter();
                let dst_args = fnctx.pos.func.dfg.ebb_params(dst).iter();

                for (inst_arg, dst_arg) in inst_dst_args.clone().zip(dst_args) {
                    //println!("inst_arg:{:?} dst_arg:{:?}", inst_arg, dst_arg);
                    let phi = fnctx.values[dst_arg].into_phi_value();
                    let inst_arg = fnctx.values[inst_arg].into_int_value();

                    phi.add_incoming(&[(&inst_arg, &curr_bb)]);

                }

                self.builder.position_at_end(&new_bb);
            }
            _ => println!("Unknown branch op: {:?}", opcode),
        }
    }

    fn translate_jump(
        &mut self,
        _inst: ir::Inst,
        opcode: ir::Opcode,
        dst: ir::Ebb,
        args: &ir::ValueList,
        fnctx: &mut FunctionContext,
    ) {
        match opcode {
            ir::Opcode::Jump => {
                let dst_bb = self.data.entry_bb_for_ebb(dst, fnctx.func);
                let curr_bb = self.builder.get_insert_block().unwrap();
                self.builder.build_unconditional_branch(dst_bb);

                let inst_dst_args = &args.as_slice(fnctx.entity_pool()).iter();
                let dst_args = fnctx.pos.func.dfg.ebb_params(dst).iter();

                for (inst_arg, dst_arg) in inst_dst_args.clone().zip(dst_args) {
                    //println!("inst_arg:{:?} dst_arg:{:?}", inst_arg, dst_arg);
                    let phi = fnctx.values[dst_arg].into_phi_value();
                    let inst_arg = fnctx.values[inst_arg].into_int_value();

                    phi.add_incoming(&[(&inst_arg, &curr_bb)]);

                }
            }
            _ => println!("Unknown jump op: {:?}", opcode),
        }
    }

    fn translate_multiary(
        &mut self,
        _inst: ir::Inst,
        opcode: ir::Opcode,
        _args: &ir::ValueList,
        _fnctx: &mut FunctionContext,
    ) {
        match opcode {
            ir::Opcode::Return => {
                self.builder.build_return(None);
            }
            _ => println!("Unknown jump op: {:?}", opcode),
        }
    }


    fn translate_unary_imm(
        &mut self,
        inst: ir::Inst,
        opcode: ir::Opcode,
        imm: Imm64,
        fnctx: &mut FunctionContext,
    ) {
        match opcode {
            ir::Opcode::Iconst => {
                // Convert from Cton to native Rust int
                let result = fnctx.pos.func.dfg.inst_results(inst)[0];
                let imm_conv: i64 = imm.into();
                let native_imm = imm_conv as u64;

                // Convert from native to LLVM int
                let i64_ty = self.context.i64_type();
                let imm_const = i64_ty.const_int(native_imm, false);

                // Store the value
                fnctx.insert_value(result, TV::int(imm_const));
            }
            _ => panic!("Unknown opcode: {:?}", opcode),
        }
    }

    fn create_func(&self, cton_func: &ir::Function) -> FunctionValue {
        let func_name = match cton_func.name {
            ir::ExternalName::TestCase { length, ref ascii } => &ascii[0..length as usize],
            _ => panic!("unknown externname type: {:?}", cton_func.name),
        };
        let func_name = str::from_utf8(func_name).expect("utf8 str");

        let ret_type = self.context.void_type();
        let i64_ty = &self.context.i64_type(); // TODO: translate types
        let mut arg_types: Vec<&types::BasicType> = Vec::new();
        for _ in &cton_func.signature.params {
            arg_types.push(i64_ty);
        }
        //let arg_types: &[&types::IntType] = &arg_types;
        let fn_type = ret_type.fn_type(&arg_types, false);
        self.module.add_function(func_name, &fn_type, None)
    }
}

impl TranslatorData {
    fn entry_bb_for_ebb(&mut self, ebb: ir::Ebb, func: FunctionValue) -> &BasicBlock {
        match self.ebb_to_entry.entry(ebb) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let new_bb = func.append_basic_block("");
                entry.insert(new_bb)
            }
        }
    }
}


#[derive(Debug)]
enum TV {
    Int(IntValue),
    Phi(PhiValue),
}

impl TV {
    fn phi(v: PhiValue) -> TV {
        TV::Phi(v)
    }

    fn int(v: IntValue) -> TV {
        TV::Int(v)
    }

    fn into_int_value(&self) -> IntValue {
        match self {
            &TV::Int(x) => x,
            &TV::Phi(x) => x.as_basic_value().into_int_value(),
        }
    }

    fn into_phi_value(&self) -> PhiValue {
        match self {
            &TV::Phi(x) => x,
            x => panic!("Expected phi, got {:?}", x),
        }
    }
}
