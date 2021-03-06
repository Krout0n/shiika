use crate::ast::*;
use crate::code_gen::CodeGen;
use crate::error::Error;
use crate::hir;
use crate::hir::class_dict::ClassDict;
use crate::hir::hir_maker_context::*;
use crate::hir::method_dict::MethodDict;
use crate::hir::*;
use crate::names;
use crate::type_checking;

#[derive(Debug)]
pub struct HirMaker {
    /// List of classes found so far
    pub(super) class_dict: ClassDict,
    /// List of methods found so far
    pub(super) method_dict: MethodDict,
    /// List of constants found so far
    pub(super) constants: HashMap<ConstFullname, TermTy>,
    pub(super) const_inits: Vec<HirExpression>,
    /// List of string literals found so far
    pub(super) str_literals: Vec<String>,
    /// Stack of ctx
    pub(super) ctx_stack: Vec<HirMakerContext>,
    /// Gensym (currently used by array literals)
    gensym_ct: usize,
    /// Counter to give unique name for lambdas
    pub(super) lambda_ct: usize,
}

pub fn make_hir(ast: ast::Program, corelib: Corelib) -> Result<Hir, Error> {
    let class_dict = class_dict::create(&ast, corelib.sk_classes)?;
    let mut hir = convert_program(class_dict, ast)?;

    // While corelib classes are included in `class_dict`,
    // corelib methods are not. Here we need to add them manually
    hir.add_methods(corelib.sk_methods);

    Ok(hir)
}

fn convert_program(class_dict: ClassDict, prog: ast::Program) -> Result<Hir, Error> {
    let mut hir_maker = HirMaker::new(class_dict);
    hir_maker.register_class_consts();
    let main_exprs = hir_maker.convert_toplevel_items(&prog.toplevel_items)?;
    Ok(hir_maker.extract_hir(main_exprs))
}

impl HirMaker {
    fn new(class_dict: ClassDict) -> HirMaker {
        HirMaker {
            class_dict,
            method_dict: MethodDict::new(),
            constants: HashMap::new(),
            const_inits: vec![],
            str_literals: vec![],
            ctx_stack: vec![],
            gensym_ct: 0,
            lambda_ct: 0,
        }
    }

    /// Destructively convert self to Hir
    fn extract_hir(&mut self, main_exprs: HirExpressions) -> Hir {
        // Extract data from self
        let sk_classes = std::mem::replace(&mut self.class_dict.sk_classes, HashMap::new());
        let sk_methods = std::mem::take(&mut self.method_dict.sk_methods);
        let mut constants = HashMap::new();
        std::mem::swap(&mut constants, &mut self.constants);
        let mut str_literals = vec![];
        std::mem::swap(&mut str_literals, &mut self.str_literals);
        let mut const_inits = vec![];
        std::mem::swap(&mut const_inits, &mut self.const_inits);

        // Register void
        constants.insert(const_fullname("::Void"), ty::raw("Void"));

        Hir {
            sk_classes,
            sk_methods,
            constants,
            str_literals,
            const_inits,
            main_exprs,
        }
    }

    fn register_class_consts(&mut self) {
        // mem::take is needed to avoid compile error
        let classes = std::mem::take(&mut self.class_dict.sk_classes);
        for (name, class) in &classes {
            if !name.is_meta() && !class.const_is_obj {
                self.register_class_const(name);
            }
        }
        self.class_dict.sk_classes = classes;
    }

    /// Register a constant that holds a class
    fn register_class_const(&mut self, fullname: &ClassFullname) {
        let instance_ty = ty::raw(&fullname.0);
        let class_ty = instance_ty.meta_ty();
        let const_name = const_fullname(&format!("::{}", &fullname.0));

        // eg. Constant `A` holds the class A
        self.constants.insert(const_name.clone(), class_ty);
        // eg. "A"
        let idx = self.register_string_literal(&fullname.0);
        // eg. A = Meta:A.new
        let op = Hir::assign_const(const_name, Hir::class_literal(fullname.clone(), idx));
        self.const_inits.push(op);
    }

    fn convert_toplevel_items(
        &mut self,
        items: &[ast::TopLevelItem],
    ) -> Result<HirExpressions, Error> {
        let mut main_exprs = vec![];
        // Contains local vars defined at toplevel
        self.push_ctx(HirMakerContext::toplevel());
        for item in items {
            match item {
                ast::TopLevelItem::Def(def) => {
                    self.process_toplevel_def(&def)?;
                }
                ast::TopLevelItem::Expr(expr) => {
                    main_exprs.push(self.convert_expr(&expr)?);
                }
            }
        }
        self.pop_ctx();
        Ok(HirExpressions::new(main_exprs))
    }

    fn process_toplevel_def(&mut self, def: &ast::Definition) -> Result<(), Error> {
        match def {
            // Extract instance/class methods
            ast::Definition::ClassDefinition { name, defs, .. } => {
                let full = name.add_namespace("");
                self.collect_sk_methods(&full, defs)?;
            }
            ast::Definition::ConstDefinition { name, expr } => {
                self.register_const(name, expr)?;
            }
            _ => panic!("should be checked in hir::class_dict"),
        }
        Ok(())
    }

    /// Extract instance/class methods and constants
    fn collect_sk_methods(
        &mut self,
        fullname: &ClassFullname,
        defs: &[ast::Definition],
    ) -> Result<(), Error> {
        self.register_meta_ivar(&fullname)?;
        self.process_defs(defs, &fullname)?;
        Ok(())
    }

    fn register_meta_ivar(&mut self, name: &ClassFullname) -> Result<(), Error> {
        let mut meta_ivars = HashMap::new();
        meta_ivars.insert(
            "name".to_string(),
            SkIVar {
                name: "name".to_string(),
                idx: 0,
                ty: ty::raw("String"),
                readonly: true,
            },
        );
        self.define_ivars(&name.meta_name(), meta_ivars, &[])?;
        Ok(())
    }

    /// Process each method def and const def
    fn process_defs(
        &mut self,
        defs: &[ast::Definition],
        fullname: &ClassFullname,
    ) -> Result<(), Error> {
        let meta_name = fullname.meta_name();
        let mut ctx = HirMakerContext::class_ctx(&fullname);

        // Add `#initialize`
        let mut own_ivars = HashMap::default();
        if let Some(ast::Definition::InstanceMethodDefinition {
            sig, body_exprs, ..
        }) = defs.iter().find(|d| d.is_initializer())
        {
            let (sk_method, found_ivars) =
                self.create_initialize(&mut ctx, &fullname, &sig.name, &body_exprs)?;
            self.method_dict.add_method(&fullname, sk_method);
            own_ivars = found_ivars;
        }
        self.define_ivars(fullname, own_ivars, defs)?;

        // Add `.new`
        if has_new(&fullname) {
            self.method_dict
                .add_method(&meta_name, self.create_new(&fullname)?);
        }

        for def in defs.iter().filter(|d| !d.is_initializer()) {
            match def {
                ast::Definition::InstanceMethodDefinition {
                    sig, body_exprs, ..
                } => {
                    let method =
                        self.convert_method_def(&ctx, &fullname, &sig.name, &body_exprs)?;
                    self.method_dict.add_method(&fullname, method);
                }
                ast::Definition::ClassMethodDefinition {
                    sig, body_exprs, ..
                } => {
                    let method =
                        self.convert_method_def(&ctx, &meta_name, &sig.name, &body_exprs)?;
                    self.method_dict.add_method(&meta_name, method);
                }
                ast::Definition::ConstDefinition { name, expr } => {
                    self.register_const(name, expr)?;
                }
                ast::Definition::ClassDefinition { name, defs, .. } => {
                    let full = name.add_namespace(&fullname.0);
                    self.collect_sk_methods(&full, defs)?;
                }
            }
        }
        Ok(())
    }

    /// Create the `initialize` method
    /// Also, define ivars
    fn create_initialize(
        &mut self,
        ctx: &mut HirMakerContext,
        class_fullname: &ClassFullname,
        name: &MethodFirstname,
        body_exprs: &[AstExpression],
    ) -> Result<(SkMethod, SkIVars), Error> {
        let super_ivars = self
            .class_dict
            .get_superclass(class_fullname)
            .map(|super_cls| super_cls.ivars.clone());
        self.convert_method_def_(ctx, class_fullname, name, body_exprs, true, super_ivars)
    }

    /// Define ivars of a class
    /// Also, define accessors
    fn define_ivars(
        &mut self,
        clsname: &ClassFullname,
        own_ivars: SkIVars,
        defs: &[ast::Definition],
    ) -> Result<(), Error> {
        self.class_dict.define_ivars(clsname, own_ivars.clone())?;
        self.define_accessors(clsname, own_ivars, defs);
        Ok(())
    }

    /// Create .new
    fn create_new(&self, class_fullname: &ClassFullname) -> Result<SkMethod, Error> {
        let class_fullname = class_fullname.clone();
        let (initialize_name, initialize_params, init_cls_name) =
            self.find_initialize(&class_fullname.instance_ty())?;
        let instance_ty = ty::raw(&class_fullname.0);
        let meta_name = class_fullname.meta_name();
        let need_bitcast = init_cls_name != class_fullname;
        let arity = initialize_params.len();

        let new_body = move |code_gen: &CodeGen, function: &inkwell::values::FunctionValue| {
            // Allocate memory
            let obj = code_gen.allocate_sk_obj(&class_fullname, "addr");

            // Call initialize
            let initialize = code_gen
                .module
                .get_function(&initialize_name.full_name)
                .unwrap_or_else(|| panic!("[BUG] function `{}' not found", &initialize_name));
            let mut addr = obj;
            if need_bitcast {
                let ances_type = code_gen
                    .llvm_struct_types
                    .get(&init_cls_name)
                    .expect("ances_type not found")
                    .ptr_type(inkwell::AddressSpace::Generic);
                addr = code_gen
                    .builder
                    .build_bitcast(addr, ances_type, "obj_as_super");
            }
            let args = (0..=arity)
                .map(|i| {
                    if i == 0 {
                        addr
                    } else {
                        function.get_params()[i]
                    }
                })
                .collect::<Vec<_>>();
            code_gen.builder.build_call(initialize, &args, "");

            code_gen.builder.build_return(Some(&obj));
            Ok(())
        };

        Ok(SkMethod {
            signature: hir::signature::signature_of_new(
                &meta_name,
                initialize_params,
                &instance_ty,
            ),
            body: SkMethodBody::RustClosureMethodBody {
                boxed_gen: Box::new(new_body),
            },
        })
    }

    fn find_initialize(
        &self,
        class: &TermTy,
    ) -> Result<(MethodFullname, Vec<MethodParam>, ClassFullname), Error> {
        let (sig, found_cls) = self
            .class_dict
            .lookup_method(&class, &method_firstname("initialize"))?;
        Ok((
            names::method_fullname(&found_cls, "initialize"),
            sig.params,
            found_cls,
        ))
    }

    /// Register a constant
    pub(super) fn register_const(
        &mut self,
        name: &ConstFirstname,
        expr: &AstExpression,
    ) -> Result<ConstFullname, Error> {
        let ctx = self.ctx();
        // TODO: resolve name using ctx
        let fullname = const_fullname(&format!("{}::{}", ctx.namespace.0, &name.0));
        let hir_expr = self.convert_expr(expr)?;
        self.constants.insert(fullname.clone(), hir_expr.ty.clone());
        let op = Hir::assign_const(fullname.clone(), hir_expr);
        self.const_inits.push(op);
        Ok(fullname)
    }

    fn convert_method_def(
        &mut self,
        ctx: &HirMakerContext,
        class_fullname: &ClassFullname,
        name: &MethodFirstname,
        body_exprs: &[AstExpression],
    ) -> Result<SkMethod, Error> {
        let (sk_method, _ivars) =
            self.convert_method_def_(ctx, class_fullname, name, body_exprs, false, None)?;
        Ok(sk_method)
    }

    /// Create a SkMethod and return it with ctx.iivars
    fn convert_method_def_(
        &mut self,
        ctx: &HirMakerContext,
        class_fullname: &ClassFullname,
        name: &MethodFirstname,
        body_exprs: &[AstExpression],
        is_initializer: bool,
        super_ivars: Option<SkIVars>,
    ) -> Result<(SkMethod, HashMap<String, SkIVar>), Error> {
        // MethodSignature is built beforehand by class_dict::new
        let err = format!(
            "[BUG] signature not found ({}/{}/{:?})",
            class_fullname, name, self.class_dict
        );
        let signature = self
            .class_dict
            .find_method(class_fullname, name)
            .expect(&err)
            .clone();

        self.push_ctx(HirMakerContext::method_ctx(
            ctx,
            &signature,
            is_initializer,
            super_ivars.unwrap_or_else(|| HashMap::new()),
        ));
        let body_exprs = self.convert_exprs(body_exprs)?;
        let iivars = self.pop_ctx().iivars;
        type_checking::check_return_value(&signature, &body_exprs.ty)?;

        let body = SkMethodBody::ShiikaMethodBody { exprs: body_exprs };
        Ok((SkMethod { signature, body }, iivars))
    }

    /// Generate unique variable name
    pub(super) fn gensym(&mut self) -> String {
        self.gensym_ct += 1;
        // Start from space so that it won't collide with user vars
        format!(" tmp{}", self.gensym_ct)
    }
}

// Whether the class has .new
fn has_new(fullname: &ClassFullname) -> bool {
    // TODO: maybe more?
    // At least these two must be excluded (otherwise wrong .ll is generated)
    if fullname.0 == "Int" || fullname.0 == "Float" {
        return false;
    }
    true
}
