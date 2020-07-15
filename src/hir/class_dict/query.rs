use crate::error;
use crate::error::*;
use crate::hir::*;
use crate::hir::class_dict::class_dict::ClassDict;
use crate::ty::*;
use crate::names::*;

impl ClassDict {
    /// Find a method from class name and first name
    pub fn find_method(&self, class_fullname: &ClassFullname, method_name: &MethodFirstname) -> Option<&MethodSignature> {
        self.sk_classes.get(class_fullname).and_then(|class| class.method_sigs.get(method_name))
    }

    /// Similar to find_method, but lookup into superclass if not in the class.
    /// Returns Err if not found.
    pub fn lookup_method(&self,
                         class_fullname: &ClassFullname,
                         method_name: &MethodFirstname)
                         -> Result<(&MethodSignature, ClassFullname), Error> {
        self.lookup_method_(class_fullname, class_fullname, method_name)
    }
    fn lookup_method_(&self,
                      receiver_class_fullname: &ClassFullname,
                      class_fullname: &ClassFullname,
                      method_name: &MethodFirstname)
                         -> Result<(&MethodSignature, ClassFullname), Error> {
        if let Some(sig) = self.find_method(class_fullname, method_name) {
            Ok((sig, class_fullname.clone()))
        }
        else {
            // Look up in superclass
            let sk_class = self.find_class(class_fullname)
                .unwrap_or_else(|| panic!("[BUG] lookup_method: asked to find `{}' but class `{}' not found", &method_name.0, &class_fullname.0));
            if let Some(super_name) = &sk_class.superclass_fullname {
                self.lookup_method_(receiver_class_fullname, super_name, method_name)
            }
            else {
                Err(error::program_error(&format!("method {:?} not found on {:?}", method_name, receiver_class_fullname)))
            }
        }
    }

    /// Find a class
    pub fn find_class(&self, class_fullname: &ClassFullname) -> Option<&SkClass> {
        self.sk_classes.get(class_fullname)
    }

    /// Find a class. Panic if not found
    pub fn get_class(&self,
                     class_fullname: &ClassFullname,
                     dbg_name: &str) -> &SkClass {
        self.find_class(class_fullname)
            .unwrap_or_else(|| panic!("[BUG] {}: class `{}' not found", &dbg_name, &class_fullname.0))
    }

    /// Find a class. Panic if not found
    pub fn get_class_mut(&mut self,
                         class_fullname: &ClassFullname,
                         dbg_name: &str) -> &mut SkClass {
        self.sk_classes.get_mut(&class_fullname)
            .unwrap_or_else(|| panic!("[BUG] {}: class `{}' not found", &dbg_name, &class_fullname.0))
    }

    /// Return true if there is a class of the name
    pub fn class_exists(&self, class_fullname: &str) -> bool {
        self.sk_classes.contains_key(&ClassFullname(class_fullname.to_string()))
    }

    /// Find the superclass
    /// Return None if the class is `Object`
    pub fn get_superclass(&self, classname: &ClassFullname) -> Option<&SkClass> {
        let cls = self.get_class(&classname, "ClassDict::get_superclass");
        cls.superclass_fullname.as_ref().map(|super_name| {
            self.get_class(&super_name, "ClassDict::get_superclass")
        })
    }

    /// Return supertype of `ty`
    pub fn supertype_of(&self, ty: &TermTy) -> Option<TermTy> {
        ty.supertype(self)
    }

    /// Return ancestor types of `ty`, including itself.
    pub fn ancestor_types(&self, ty: &TermTy) -> Vec<TermTy> {
        let mut v = vec![];
        let mut t = Some(ty.clone());
        while t.is_some() {
            v.push(t.unwrap());
            t = self.supertype_of(&v.last().unwrap())
        }
        v
     }

    pub fn find_ivar(&self,
                     classname: &ClassFullname,
                     ivar_name: &str) -> Option<&SkIVar> {
        let class = self.sk_classes.get(&classname)
            .unwrap_or_else(|| panic!("[BUG] ClassDict::find_ivar: class `{}' not found", &classname));
        class.ivars.get(ivar_name)
    }
}
