use crate::compile::engine::df::scan::CubeScanNode;
use crate::compile::engine::provider::CubeContext;
use crate::mysql::AuthContext;
use crate::CubeError;
use cubeclient::models::V1LoadRequestQuery;
use datafusion::arrow::datatypes::DataType;
use datafusion::catalog::TableReference;
use datafusion::logical_plan::window_frames::WindowFrame;
use datafusion::logical_plan::Column;
use datafusion::logical_plan::{
    build_join_schema, exprlist_to_fields, normalize_cols, DFField, DFSchema, DFSchemaRef, Expr,
    JoinConstraint, JoinType, LogicalPlan, Operator, Partitioning,
};
use datafusion::physical_plan::aggregates::AggregateFunction;
use datafusion::physical_plan::functions::BuiltinScalarFunction;
use datafusion::physical_plan::udaf::AggregateUDF;
use datafusion::physical_plan::udf::ScalarUDF;
use datafusion::physical_plan::window_functions::WindowFunction;
use datafusion::scalar::ScalarValue;
use datafusion::sql::parser::FileType;
use datafusion::sql::planner::ContextProvider;
use egg::{rewrite, CostFunction, Language, Subst};
use egg::{EGraph, Extractor, Id, RecExpr, Rewrite, Runner};
use itertools::Itertools;
use std::ops::Index;
use std::str::FromStr;
use std::sync::Arc;

#[macro_export]
macro_rules! plan_to_language {
    ($(#[$meta:meta])* $vis:vis enum $name:ident $variants:tt) => {
        $crate::__plan_to_language!($(#[$meta])* $vis enum $name $variants -> {});
    };
}

macro_rules! variant_field_struct {
    ($variant:ident, $var_field:ident, String) => {
        paste::item! {
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
            pub struct [<$variant $var_field:camel>](String);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    let prefix = format!("{}:", std::stringify!([<$variant $var_field:camel>]));
                    if s.starts_with(&prefix) {
                        return Ok([<$variant $var_field:camel>](s.replace(&prefix, "")));
                    }
                    Err(CubeError::internal(format!("Can't convert {}. Should start with '{}'", s, prefix)))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }
        }
    };

    ($variant:ident, $var_field:ident, Option<String>) => {
        paste::item! {
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
            pub struct [<$variant $var_field:camel>](Option<String>);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(_s: &str) -> Result<Self, Self::Err> {
                    Err(CubeError::internal("Conversion from string is not supported".to_string()))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }
        }
    };

    ($variant:ident, $var_field:ident, Column) => {
        paste::item! {
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
            pub struct [<$variant $var_field:camel>](Column);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(_s: &str) -> Result<Self, Self::Err> {
                    Err(CubeError::internal("Conversion from string is not supported".to_string()))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }
        }
    };

    ($variant:ident, $var_field:ident, Vec<Column>) => {
        paste::item! {
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
            pub struct [<$variant $var_field:camel>](Vec<Column>);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(_s: &str) -> Result<Self, Self::Err> {
                    Err(CubeError::internal("Conversion from string is not supported".to_string()))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }
        }
    };

    ($variant:ident, $var_field:ident, Arc<AggregateUDF>) => {
        paste::item! {
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
            pub struct [<$variant $var_field:camel>](String);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(_s: &str) -> Result<Self, Self::Err> {
                    Err(CubeError::internal("Conversion from string is not supported".to_string()))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }
        }
    };

    ($variant:ident, $var_field:ident, Arc<ScalarUDF>) => {
        paste::item! {
            #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
            pub struct [<$variant $var_field:camel>](String);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(_s: &str) -> Result<Self, Self::Err> {
                    Err(CubeError::internal("Conversion from string is not supported".to_string()))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }
        }
    };

    ($variant:ident, $var_field:ident, AggregateFunction) => {
        variant_field_struct!(
            @enum_struct $variant, $var_field, { AggregateFunction } -> {
                AggregateFunction::Count => "Count",
                AggregateFunction::Sum => "Sum",
                AggregateFunction::Min => "Min",
                AggregateFunction::Max => "Max",
                AggregateFunction::Avg => "Avg",
                AggregateFunction::ApproxDistinct => "ApproxDistinct",
            }
        );
    };

    ($variant:ident, $var_field:ident, Operator) => {
        variant_field_struct!(
            @enum_struct $variant, $var_field, { Operator } -> {
                Operator::Eq => "=",
                Operator::NotEq => "!=",
                Operator::Lt => "<",
                Operator::LtEq => "<=",
                Operator::Gt => ">",
                Operator::GtEq => ">=",
                Operator::Plus => "+",
                Operator::Minus => "-",
                Operator::Multiply => "*",
                Operator::Divide => "/",
                Operator::Modulo => "%",
                Operator::And => "AND",
                Operator::Or => "OR",
                Operator::Like => "LIKE",
                Operator::NotLike => "NOT_LIKE",
                Operator::RegexMatch => "~",
                Operator::RegexIMatch => "~*",
                Operator::RegexNotMatch => "!~",
                Operator::RegexNotIMatch => "!~*",
                Operator::IsDistinctFrom => "IS_DISTINCT_FROM",
                Operator::IsNotDistinctFrom => "IS_NOT_DISTINCT_FROM",
            }
        );
    };

    ($variant:ident, $var_field:ident, JoinType) => {
        variant_field_struct!(
            @enum_struct $variant, $var_field, { JoinType } -> {
                JoinType::Inner => "Inner",
                JoinType::Left => "Left",
                JoinType::Right => "Right",
                JoinType::Full => "Full",
                JoinType::Semi => "Semi",
                JoinType::Anti => "Anti",
            }
        );
    };

    ($variant:ident, $var_field:ident, JoinConstraint) => {
        variant_field_struct!(
            @enum_struct $variant, $var_field, { JoinConstraint } -> {
                JoinConstraint::On => "On",
                JoinConstraint::Using => "Using",
            }
        );
    };

    (@enum_struct $variant:ident, $var_field:ident, { $var_field_type:ty } -> {$($variant_type:ty => $name:literal,)*}) => {
        paste::item! {
            #[derive(Debug, Clone)]
            pub struct [<$variant $var_field:camel>]($var_field_type);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    match s {
                        $($name => Ok([<$variant $var_field:camel>]($variant_type)),)*
                        x => Err(CubeError::internal(format!("{} can't be matched against {}", x, std::stringify!($var_field_type))))
                    }
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let name = match self.0 {
                        $($variant_type => $name,)*
                    };
                    write!(f, "{}", name)
                }
            }

            impl core::cmp::Ord for [<$variant $var_field:camel>] {
                fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                    let name = match self.0 {
                        $($variant_type => $name,)*
                    };
                    let other_name = match other.0 {
                        $($variant_type => $name,)*
                    };
                    name.cmp(other_name)
                }
            }

            impl core::cmp::PartialOrd for [<$variant $var_field:camel>] {
                fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                    let name = match self.0 {
                        $($variant_type => $name,)*
                    };
                    let other_name = match other.0 {
                        $($variant_type => $name,)*
                    };
                    name.partial_cmp(other_name)
                }
            }

            impl core::hash::Hash for [<$variant $var_field:camel>] {
                fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                    std::mem::discriminant(&self.0).hash(state);
                }
            }

            impl core::cmp::PartialEq for [<$variant $var_field:camel>] {
                fn eq(&self, other: &[<$variant $var_field:camel>]) -> bool {
                    let name = match self.0 {
                        $($variant_type => $name,)*
                    };
                    let other_name = match other.0 {
                        $($variant_type => $name,)*
                    };
                    name == other_name
                }
            }

            impl core::cmp::Eq for [<$variant $var_field:camel>] {}
        }
    };

    ($variant:ident, $var_field:ident, $var_field_type:ty) => {
        paste::item! {
            #[derive(Debug, PartialOrd, Clone)]
            pub struct [<$variant $var_field:camel>]($var_field_type);

            impl FromStr for [<$variant $var_field:camel>] {
                type Err = CubeError;
                fn from_str(_s: &str) -> Result<Self, Self::Err> {
                    Err(CubeError::internal("Conversion from string is not supported".to_string()))
                }
            }

            impl std::fmt::Display for [<$variant $var_field:camel>] {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self)
                }
            }

            impl core::cmp::Ord for [<$variant $var_field:camel>] {
                fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                    self.partial_cmp(&other).unwrap()
                }
            }

            impl core::hash::Hash for [<$variant $var_field:camel>] {
                fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
                    std::mem::discriminant(&self.0).hash(state);
                }
            }

            impl core::cmp::PartialEq for [<$variant $var_field:camel>] {
                fn eq(&self, other: &[<$variant $var_field:camel>]) -> bool {
                    self.0 == other.0
                }
            }

            impl core::cmp::Eq for [<$variant $var_field:camel>] {}
        }
    };
}

#[macro_export]
macro_rules! __plan_to_language {
    (@define_language $(#[$meta:meta])* $vis:vis enum $name:ident {} ->
     $decl:tt {$($matches:tt)*} $children:tt $children_mut:tt
     $display:tt {$($from_op:tt)*} {$($type_decl:tt)*}
    ) => { paste::item! {
        $(#[$meta])*
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
        $vis enum $name $decl

        $($type_decl)*

        impl egg::Language for $name {
            #[inline(always)]
            fn matches(&self, other: &Self) -> bool {
                ::std::mem::discriminant(self) == ::std::mem::discriminant(other) &&
                match (self, other) { $($matches)* _ => false }
            }

            fn children(&self) -> &[egg::Id] { match self $children }
            fn children_mut(&mut self) -> &mut [egg::Id] { match self $children_mut }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                // We need to pass `f` to the match expression for hygiene
                // reasons.
                match (self, f) $display
            }
        }

        impl egg::FromOp for $name {
            type Error = egg::FromOpError;

            fn from_op(op: &str, children: ::std::vec::Vec<egg::Id>) -> ::std::result::Result<Self, Self::Error> {
                match (op, children) {
                    $($from_op)*
                    (op, children) => Err(egg::FromOpError::new(op, children)),
                }
            }
        }
    }};

    (@define_language $(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident ($ids:ty),
         $($variants:tt)*
     } ->
     { $($decl:tt)* } { $($matches:tt)* } { $($children:tt)* } { $($children_mut:tt)* }
     { $($display:tt)* } { $($from_op:tt)* } { $($type_decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            @define_language
            $(#[$meta])* $vis enum $name
            { $($variants)* } ->
            { $($decl)*          $variant($ids), }
            { $($matches)*       ($name::$variant(l), $name::$variant(r)) => egg::LanguageChildren::len(l) == egg::LanguageChildren::len(r), }
            { $($children)*      $name::$variant(ids) => egg::LanguageChildren::as_slice(ids), }
            { $($children_mut)*  $name::$variant(ids) => egg::LanguageChildren::as_mut_slice(ids), }
            { $($display)*       ($name::$variant(..), f) => f.write_str(std::stringify!($variant)), }
            { $($from_op)*       (op, children) if op == std::stringify!($variant) && <$ids as egg::LanguageChildren>::can_be_length(children.len()) => {
                  let children = <$ids as egg::LanguageChildren>::from_vec(children);
                  Ok($name::$variant(children))
              },
            }
            { $($type_decl)* }
        );
    };

    (@define_language $(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident $var_field:ident ($ids:ty),
         $($variants:tt)*
     } ->
     { $($decl:tt)* } { $($matches:tt)* } { $($children:tt)* } { $($children_mut:tt)* }
     { $($display:tt)* } { $($from_op:tt)* } { $($type_decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            @define_language
            $(#[$meta])* $vis enum $name
            { $($variants)* } ->
            { $($decl)*          [<$variant $var_field:camel>]($ids), }
            { $($matches)*       ($name::[<$variant $var_field:camel>](l), $name::[<$variant $var_field:camel>](r)) => egg::LanguageChildren::len(l) == egg::LanguageChildren::len(r), }
            { $($children)*      $name::[<$variant $var_field:camel>](ids) => egg::LanguageChildren::as_slice(ids), }
            { $($children_mut)*  $name::[<$variant $var_field:camel>](ids) => egg::LanguageChildren::as_mut_slice(ids), }
            { $($display)*       ($name::[<$variant $var_field:camel>](..), f) => f.write_str(std::stringify!([<$variant $var_field:camel>])), }
            { $($from_op)*       (op, children) if op == std::stringify!([<$variant $var_field:camel>]) && <$ids as egg::LanguageChildren>::can_be_length(children.len()) => {
                  let children = <$ids as egg::LanguageChildren>::from_vec(children);
                  Ok($name::[<$variant $var_field:camel>](children))
              },
            }
            { $($type_decl)* }
        );
    };

    (@define_language $(#[$meta:meta])* $vis:vis enum $name:ident
     {
         @data $variant:ident $var_field:ident ($data:ty),
         $($variants:tt)*
     } ->
     { $($decl:tt)* } { $($matches:tt)* } { $($children:tt)* } { $($children_mut:tt)* }
     { $($display:tt)* } { $($from_op:tt)* } { $($type_decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            @define_language $(#[$meta])* $vis enum $name
            { $($variants)* } ->
            { $($decl)*          [<$variant $var_field:camel>]([<$variant $var_field:camel>]), }
            { $($matches)*       ($name::[<$variant $var_field:camel>](data1), $name::[<$variant $var_field:camel>](data2)) => data1 == data2, }
            { $($children)*      $name::[<$variant $var_field:camel>](_data) => &[], }
            { $($children_mut)*  $name::[<$variant $var_field:camel>](_data) => &mut [], }
            { $($display)*       ($name::[<$variant $var_field:camel>](data), f) => ::std::fmt::Display::fmt(data, f), }
            { $($from_op)*       (op, children) if op.parse::<[<$variant $var_field:camel>]>().is_ok() && children.is_empty() => Ok($name::[<$variant $var_field:camel>](op.parse().unwrap())), }
            {
                $($type_decl)*
                variant_field_struct!($variant, $var_field, $data);
            }
        );
    };

    // Here transform from variants to @define_language begins.
    // It transforms variant fields to language variants.
    // The reason it's so complex and not part of @define_language is we can't call macros inside
    // enum declaration block, i.e. we can't do { $($decl)* $(enum_decl!($var_field, $var_field_type),)* }.

    ($(#[$meta:meta])* $vis:vis enum $name:ident {} ->
     $decl:tt
    ) => {
        $crate::__plan_to_language! {
            @define_language
            $(#[$meta])*
            $vis enum $name $decl
            -> {} {} {} {} {} {} {}
        }
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            { $($variants)* } ->
            { $($decl)* $variant([egg::Id; $variant_size]), }
        );
    };

    // Reference rules

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Arc<LogicalPlan>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Expr,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Arc<Expr>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Box<Expr>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* }
        );
    };

    // References inside container

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Vec<LogicalPlan>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* $variant $var_field (Vec<egg::Id>), }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Vec<Expr>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* $variant $var_field (Vec<egg::Id>), }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Vec<(Box<Expr>, Box<Expr>)>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* $variant $var_field (Vec<egg::Id>), }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : Option<Box<Expr>>,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* $variant $var_field (Vec<egg::Id>), }
        );
    };

    // Skip schema as it isn't part of rewrite. TODO remove?

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : DFSchemaRef,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            @variant_size $variant_size:expr,
            $var_field:ident : $var_field_type:ty,
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size $variant_size + 1,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* @data $variant $var_field ($var_field_type), }
        );
    };

    ($(#[$meta:meta])* $vis:vis enum $name:ident
     {
         $variant:ident {
            $($var_fields:tt)*
         },
         $($variants:tt)*
     } ->
     { $($decl:tt)* }
    ) => {
        $crate::__plan_to_language!(
            $(#[$meta])* $vis enum $name
            {
                $variant {
                    @variant_size 0usize,
                    $($var_fields)*
                },
                $($variants)*
            } ->
            { $($decl)* }
        );
    };
}

trace_macros!(false);

plan_to_language! {
    pub enum LogicalPlanLanguage {
        Projection {
            expr: Vec<Expr>,
            input: Arc<LogicalPlan>,
            schema: DFSchemaRef,
            alias: Option<String>,
        },
        Filter {
            predicate: Expr,
            input: Arc<LogicalPlan>,
        },
        Window {
            input: Arc<LogicalPlan>,
            window_expr: Vec<Expr>,
            schema: DFSchemaRef,
        },
        Aggregate {
            input: Arc<LogicalPlan>,
            group_expr: Vec<Expr>,
            aggr_expr: Vec<Expr>,
            schema: DFSchemaRef,
        },
        Sort {
            exp: Vec<Expr>,
            input: Arc<LogicalPlan>,
        },
        Join {
            left: Arc<LogicalPlan>,
            right: Arc<LogicalPlan>,
            left_on: Vec<Column>,
            right_on: Vec<Column>,
            join_type: JoinType,
            join_constraint: JoinConstraint,
            schema: DFSchemaRef,
        },
        CrossJoin {
            left: Arc<LogicalPlan>,
            right: Arc<LogicalPlan>,
            schema: DFSchemaRef,
        },
        Repartition {
            input: Arc<LogicalPlan>,
        },
        Union {
            inputs: Vec<LogicalPlan>,
            schema: DFSchemaRef,
            alias: Option<String>,
        },
        TableScan {
            source_table_name: String,
            table_name: String,
            projection: Option<Vec<usize>>,
            projected_schema: DFSchemaRef,
            filters: Vec<Expr>,
            limit: Option<usize>,
        },
        EmptyRelation {
            produce_one_row: bool,
            schema: DFSchemaRef,
        },
        Limit {
            n: usize,
            input: Arc<LogicalPlan>,
        },
        CreateExternalTable {
            schema: DFSchemaRef,
            name: String,
            location: String,
            has_header: bool,
        },
        Extension {
            node: Arc<LogicalPlan>,
        },
        CubeScan {
            cube: Arc<LogicalPlan>,
            measures: Vec<LogicalPlan>,
            dimensions: Vec<LogicalPlan>,
            filters: Vec<LogicalPlan>,
        },
        Measure {
            name: String,
        },
        Dimension {
            name: String,
        },
        TimeDimension {
            name: String,
            granularity: String,
            dateRange: Vec<String>,
        },

        AliasExpr {
            expr: Box<Expr>,
            alias: String,
        },
        ColumnExpr {
            column: Column,
        },
        ScalarVariableExpr {
            variable: Vec<String>,
        },
        LiteralExpr { value: ScalarValue, },
        BinaryExpr {
            left: Box<Expr>,
            op: Operator,
            right: Box<Expr>,
        },
        NotExpr { expr: Box<Expr>, },
        IsNotNullExpr { expr: Box<Expr>, },
        IsNullExpr { expr: Box<Expr>, },
        NegativeExpr { expr: Box<Expr>, },
        BetweenExpr {
            expr: Box<Expr>,
            negated: bool,
            low: Box<Expr>,
            high: Box<Expr>,
        },
        CaseExpr {
            expr: Option<Box<Expr>>,
            when_then_expr: Vec<(Box<Expr>, Box<Expr>)>,
            else_expr: Option<Box<Expr>>,
        },
        CastExpr {
            expr: Box<Expr>,
            data_type: DataType,
        },
        TryCastExpr {
            expr: Box<Expr>,
            data_type: DataType,
        },
        SortExpr {
            expr: Box<Expr>,
            asc: bool,
            nulls_first: bool,
        },
        ScalarFunctionExpr {
            fun: BuiltinScalarFunction,
            args: Vec<Expr>,
        },
        ScalarUDFExpr {
            fun: Arc<ScalarUDF>,
            args: Vec<Expr>,
        },
        AggregateFunctionExpr {
            fun: AggregateFunction,
            args: Vec<Expr>,
            distinct: bool,
        },
        WindowFunctionExpr {
            fun: WindowFunction,
            args: Vec<Expr>,
            partition_by: Vec<Expr>,
            order_by: Vec<Expr>,
            window_frame: Option<WindowFrame>,
        },
        AggregateUDFExpr {
            fun: Arc<AggregateUDF>,
            args: Vec<Expr>,
        },
        InListExpr {
            expr: Box<Expr>,
            list: Vec<Expr>,
            negated: bool,
        },
        WildcardExpr {},
    }
}

trace_macros!(false);

macro_rules! add_data_node {
    ($converter:expr, $value_expr:expr, $field_variant:ident) => {
        $converter
            .graph
            .add(LogicalPlanLanguage::$field_variant($field_variant(
                $value_expr.clone(),
            )))
    };
}

macro_rules! add_expr_list_node {
    ($converter:expr, $value_expr:expr, $field_variant:ident) => {{
        let list = $value_expr
            .iter()
            .map(|expr| $converter.add_expr(expr))
            .collect::<Result<Vec<_>, _>>()?;
        if list.len() < 2 {
            $converter
                .graph
                .add(LogicalPlanLanguage::$field_variant(list))
        } else {
            let mut iter = list.into_iter().rev();
            let mut current = iter.next().unwrap();
            for i in iter {
                current = $converter
                    .graph
                    .add(LogicalPlanLanguage::$field_variant(vec![i, current]));
            }
            current
        }
    }};
}

macro_rules! add_plan_list_node {
    ($converter:expr, $value_expr:expr, $field_variant:ident) => {{
        let list = $value_expr
            .iter()
            .map(|expr| $converter.add_logical_plan(expr))
            .collect::<Result<Vec<_>, _>>()?;
        if list.len() < 2 {
            $converter
                .graph
                .add(LogicalPlanLanguage::$field_variant(list))
        } else {
            let mut iter = list.into_iter().rev();
            let mut current = iter.next().unwrap();
            for i in iter {
                current = $converter
                    .graph
                    .add(LogicalPlanLanguage::$field_variant(vec![i, current]));
            }
            current
        }
    }};
}

pub struct LogicalPlanToLanguageConverter<'a> {
    graph: EGraph<LogicalPlanLanguage, ()>,
    cube_context: CubeContext<'a>,
}

impl<'a> LogicalPlanToLanguageConverter<'a> {
    pub fn new(cube_context: CubeContext<'a>) -> Self {
        Self {
            graph: EGraph::default(),
            cube_context,
        }
    }

    pub fn add_expr(&mut self, expr: &Expr) -> Result<Id, CubeError> {
        Ok(match expr {
            Expr::Alias(expr, alias) => {
                let expr = self.add_expr(expr)?;
                let alias = add_data_node!(self, alias, AliasExprAlias);
                self.graph
                    .add(LogicalPlanLanguage::AliasExpr([expr, alias]))
            }
            Expr::Column(column) => {
                let column = add_data_node!(self, column, ColumnExprColumn);
                self.graph.add(LogicalPlanLanguage::ColumnExpr([column]))
            }
            Expr::ScalarVariable(variable) => {
                let variable = add_data_node!(self, variable, ScalarVariableExprVariable);
                self.graph
                    .add(LogicalPlanLanguage::ScalarVariableExpr([variable]))
            }
            Expr::Literal(value) => {
                let value = add_data_node!(self, value, LiteralExprValue);
                self.graph.add(LogicalPlanLanguage::LiteralExpr([value]))
            }
            Expr::BinaryExpr { left, op, right } => {
                let left = self.add_expr(left)?;
                let op = add_data_node!(self, op, BinaryExprOp);
                let right = self.add_expr(right)?;
                self.graph
                    .add(LogicalPlanLanguage::BinaryExpr([left, op, right]))
            }
            Expr::Not(expr) => {
                let expr = self.add_expr(expr)?;
                self.graph.add(LogicalPlanLanguage::NotExpr([expr]))
            }
            Expr::IsNotNull(expr) => {
                let expr = self.add_expr(expr)?;
                self.graph.add(LogicalPlanLanguage::IsNotNullExpr([expr]))
            }
            Expr::IsNull(expr) => {
                let expr = self.add_expr(expr)?;
                self.graph.add(LogicalPlanLanguage::IsNullExpr([expr]))
            }
            Expr::Negative(expr) => {
                let expr = self.add_expr(expr)?;
                self.graph.add(LogicalPlanLanguage::NegativeExpr([expr]))
            }
            Expr::Between {
                expr,
                negated,
                low,
                high,
            } => {
                let expr = self.add_expr(expr)?;
                let negated = add_data_node!(self, negated, BetweenExprNegated);
                let low = self.add_expr(low)?;
                let high = self.add_expr(high)?;
                self.graph
                    .add(LogicalPlanLanguage::BetweenExpr([expr, negated, low, high]))
            }
            Expr::Case {
                expr,
                when_then_expr,
                else_expr,
            } => {
                let expr = add_expr_list_node!(self, expr, CaseExprExpr);
                let when_then_expr = when_then_expr
                    .iter()
                    .map(|(when, then)| vec![when, then])
                    .flatten()
                    .collect::<Vec<_>>();
                let when_then_expr =
                    add_expr_list_node!(self, when_then_expr, CaseExprWhenThenExpr);
                let else_expr = add_expr_list_node!(self, else_expr, CaseExprElseExpr);
                self.graph.add(LogicalPlanLanguage::CaseExpr([
                    expr,
                    when_then_expr,
                    else_expr,
                ]))
            }
            Expr::Cast { expr, data_type } => {
                let expr = self.add_expr(expr)?;
                let data_type = add_data_node!(self, data_type, CastExprDataType);
                self.graph
                    .add(LogicalPlanLanguage::CastExpr([expr, data_type]))
            }
            Expr::TryCast { expr, data_type } => {
                let expr = self.add_expr(expr)?;
                let data_type = add_data_node!(self, data_type, TryCastExprDataType);
                self.graph
                    .add(LogicalPlanLanguage::TryCastExpr([expr, data_type]))
            }
            Expr::Sort {
                expr,
                asc,
                nulls_first,
            } => {
                let expr = self.add_expr(expr)?;
                let asc = add_data_node!(self, asc, SortExprAsc);
                let nulls_first = add_data_node!(self, nulls_first, SortExprNullsFirst);
                self.graph
                    .add(LogicalPlanLanguage::SortExpr([expr, asc, nulls_first]))
            }
            Expr::ScalarFunction { fun, args } => {
                let fun = add_data_node!(self, fun, ScalarFunctionExprFun);
                let args = add_expr_list_node!(self, args, ScalarFunctionExprArgs);

                self.graph
                    .add(LogicalPlanLanguage::ScalarFunctionExpr([fun, args]))
            }
            Expr::ScalarUDF { fun, args } => {
                let fun = add_data_node!(self, fun.name, ScalarUDFExprFun);
                let args = add_expr_list_node!(self, args, ScalarUDFExprArgs);
                self.graph
                    .add(LogicalPlanLanguage::ScalarUDFExpr([fun, args]))
            }
            Expr::AggregateFunction {
                fun,
                args,
                distinct,
            } => {
                let fun = add_data_node!(self, fun, AggregateFunctionExprFun);
                let args = add_expr_list_node!(self, args, AggregateFunctionExprArgs);
                let distinct = add_data_node!(self, distinct, AggregateFunctionExprDistinct);
                self.graph.add(LogicalPlanLanguage::AggregateFunctionExpr([
                    fun, args, distinct,
                ]))
            }
            Expr::WindowFunction {
                fun,
                args,
                partition_by,
                order_by,
                window_frame,
            } => {
                let fun = add_data_node!(self, fun, WindowFunctionExprFun);
                let args = add_expr_list_node!(self, args, WindowFunctionExprArgs);
                let partition_by =
                    add_expr_list_node!(self, partition_by, WindowFunctionExprPartitionBy);
                let order_by = add_expr_list_node!(self, order_by, WindowFunctionExprOrderBy);
                let window_frame =
                    add_data_node!(self, window_frame, WindowFunctionExprWindowFrame);

                self.graph.add(LogicalPlanLanguage::WindowFunctionExpr([
                    fun,
                    args,
                    partition_by,
                    order_by,
                    window_frame,
                ]))
            }
            Expr::AggregateUDF { fun, args } => {
                let fun = add_data_node!(self, fun.name, AggregateUDFExprFun);
                let args = add_expr_list_node!(self, args, AggregateUDFExprArgs);
                self.graph
                    .add(LogicalPlanLanguage::AggregateUDFExpr([fun, args]))
            }
            Expr::InList {
                expr,
                list,
                negated,
            } => {
                let expr = self.add_expr(expr)?;
                let list = add_expr_list_node!(self, list, InListExprList);
                let negated = add_data_node!(self, negated, InListExprNegated);
                self.graph
                    .add(LogicalPlanLanguage::InListExpr([expr, list, negated]))
            }
            Expr::Wildcard => self.graph.add(LogicalPlanLanguage::WildcardExpr([])),
        })
    }

    pub fn add_logical_plan(&mut self, plan: &LogicalPlan) -> Result<Id, CubeError> {
        Ok(match plan {
            LogicalPlan::Projection {
                expr,
                input,
                schema: _,
                alias,
            } => {
                let expr = add_expr_list_node!(self, expr, ProjectionExpr);
                let input = self.add_logical_plan(input.as_ref())?;
                let alias = add_data_node!(self, alias, ProjectionAlias);
                self.graph
                    .add(LogicalPlanLanguage::Projection([expr, input, alias]))
            }
            LogicalPlan::Filter { predicate, input } => {
                let predicate = self.add_expr(predicate)?;
                let input = self.add_logical_plan(input.as_ref())?;
                self.graph
                    .add(LogicalPlanLanguage::Filter([predicate, input]))
            }
            LogicalPlan::Window {
                input,
                window_expr,
                schema: _,
            } => {
                let input = self.add_logical_plan(input.as_ref())?;
                let window_expr = add_expr_list_node!(self, window_expr, WindowWindowExpr);
                self.graph
                    .add(LogicalPlanLanguage::Window([input, window_expr]))
            }
            LogicalPlan::Aggregate {
                input,
                group_expr,
                aggr_expr,
                schema: _,
            } => {
                let input = self.add_logical_plan(input.as_ref())?;
                let group_expr = add_expr_list_node!(self, group_expr, AggregateGroupExpr);
                let aggr_expr = add_expr_list_node!(self, aggr_expr, AggregateAggrExpr);
                self.graph.add(LogicalPlanLanguage::Aggregate([
                    input, group_expr, aggr_expr,
                ]))
            }
            LogicalPlan::Sort { expr, input } => {
                let expr = add_expr_list_node!(self, expr, SortExp);
                let input = self.add_logical_plan(input.as_ref())?;
                self.graph.add(LogicalPlanLanguage::Sort([expr, input]))
            }
            LogicalPlan::Join {
                left,
                right,
                on,
                join_type,
                join_constraint,
                schema: _,
            } => {
                let left = self.add_logical_plan(left.as_ref())?;
                let right = self.add_logical_plan(right.as_ref())?;
                let left_on = on.iter().map(|(left, _)| left.clone()).collect::<Vec<_>>();
                let left_on = add_data_node!(self, left_on, JoinLeftOn);
                let right_on = on
                    .iter()
                    .map(|(_, right)| right.clone())
                    .collect::<Vec<_>>();
                let right_on = add_data_node!(self, right_on, JoinRightOn);
                let join_type = add_data_node!(self, join_type, JoinJoinType);
                let join_constraint = add_data_node!(self, join_constraint, JoinJoinConstraint);
                self.graph.add(LogicalPlanLanguage::Join([
                    left,
                    right,
                    left_on,
                    right_on,
                    join_type,
                    join_constraint,
                ]))
            }
            LogicalPlan::CrossJoin {
                left,
                right,
                schema: _,
            } => {
                let left = self.add_logical_plan(left.as_ref())?;
                let right = self.add_logical_plan(right.as_ref())?;
                self.graph
                    .add(LogicalPlanLanguage::CrossJoin([left, right]))
            }
            // TODO
            LogicalPlan::Repartition {
                input,
                partitioning_scheme: _,
            } => {
                let input = self.add_logical_plan(input.as_ref())?;
                self.graph.add(LogicalPlanLanguage::Repartition([input]))
            }
            LogicalPlan::Union {
                inputs,
                schema: _,
                alias,
            } => {
                let inputs = add_plan_list_node!(self, inputs, UnionInputs);
                let alias = add_data_node!(self, alias, UnionAlias);
                self.graph.add(LogicalPlanLanguage::Union([inputs, alias]))
            }
            LogicalPlan::TableScan {
                table_name,
                source,
                projection,
                projected_schema: _,
                filters,
                limit,
            } => {
                let source_table_name = add_data_node!(
                    self,
                    self.cube_context
                        .table_name_by_table_provider(source.clone())?,
                    TableScanSourceTableName
                );

                let table_name = add_data_node!(self, table_name, TableScanTableName);
                let projection = add_data_node!(self, projection, TableScanProjection);
                let filters = add_expr_list_node!(self, filters, TableScanFilters);
                let limit = add_data_node!(self, limit, TableScanLimit);
                self.graph.add(LogicalPlanLanguage::TableScan([
                    source_table_name,
                    table_name,
                    projection,
                    filters,
                    limit,
                ]))
            }
            LogicalPlan::EmptyRelation {
                produce_one_row,
                schema: _,
            } => {
                let produce_one_row =
                    add_data_node!(self, produce_one_row, EmptyRelationProduceOneRow);
                self.graph
                    .add(LogicalPlanLanguage::EmptyRelation([produce_one_row]))
            }
            LogicalPlan::Limit { n, input } => {
                let n = add_data_node!(self, n, LimitN);
                let input = self.add_logical_plan(input.as_ref())?;
                self.graph.add(LogicalPlanLanguage::Limit([n, input]))
            }
            LogicalPlan::CreateExternalTable { .. } => {
                panic!("CreateExternalTable is not supported");
            }
            LogicalPlan::Values { .. } => {
                panic!("Values is not supported");
            }
            LogicalPlan::Explain { .. } => {
                panic!("Explain is not supported");
            }
            LogicalPlan::Analyze { .. } => {
                panic!("Analyze is not supported");
            }
            // TODO
            LogicalPlan::Extension { node } => {
                if let Some(_cube_scan) = node.as_any().downcast_ref::<CubeScanNode>() {
                    todo!("LogicalPlanLanguage::Extension");
                    // self.graph.add(LogicalPlanLanguage::Extension([]))
                } else {
                    panic!("Unsupported extension node: {}", node.schema());
                }
            }
        })
    }

    pub fn rewrite_runner(&self) -> Runner<LogicalPlanLanguage, ()> {
        Runner::<LogicalPlanLanguage, ()>::new(Default::default())
            .with_iter_limit(100)
            .with_node_limit(10000)
            .with_egraph(self.graph.clone())
    }

    pub fn find_best_plan(
        &self,
        root: Id,
        auth_context: Arc<AuthContext>,
    ) -> Result<LogicalPlan, CubeError> {
        let runner = self.rewrite_runner();
        let rules = self.rewrite_rules();
        let runner = runner.run(rules.iter());
        let extractor = Extractor::new(&runner.egraph, BestCubePlan);
        let (_, best) = extractor.find_best(root);
        let new_root = Id::from(best.as_ref().len() - 1);
        println!("Best: {:?}", best);
        let converter = LanguageToLogicalPlanConverter {
            graph: runner.egraph,
            best_expr: best,
            cube_context: self.cube_context.clone(),
            auth_context,
        };
        converter.to_logical_plan(new_root)
    }

    pub fn rewrite_rules(&self) -> Vec<Rewrite<LogicalPlanLanguage, ()>> {
        vec![
            rewrite!("commute-eq"; "(BinaryExpr ?a = ?b)" => "(BinaryExpr ?b = ?a)"),
            rewrite!("cube-scan";
                "(TableScan ?source_table_name ?table_name ?projection ?filters ?limit)" =>
                "(Extension (CubeScan ?source_table_name CubeScanMeasures CubeScanDimensions CubeScanFilters))"
                if self.is_cube_table("?source_table_name")
            ),
            // TODO distinct should be checked to be false
            rewrite!("simple-count";
                "(Aggregate \
                    (Extension (CubeScan ?source_table_name ?measures ?dimensions ?filters)) \
                    ?group_expr \
                    (AggregateAggrExpr (AggregateFunctionExpr Count (AggregateFunctionExprArgs (LiteralExpr ?literal)) ?distinct)) \
                 )" =>
                "(Aggregate \
                    (Extension (CubeScan ?source_table_name (CubeScanMeasures ?measures (Measure MeasureName:count)) ?dimensions ?filters)) \
                    ?group_expr \
                    AggregateAggrExpr \
                 )"
                if self.has_count_measure("?source_table_name")
            ),
            rewrite!("remove-processed-aggregate";
                "(Aggregate \
                    (Extension (CubeScan ?source_table_name ?measures ?dimensions ?filters)) \
                    AggregateGroupExpr \
                    AggregateAggrExpr \
                 )" =>
                "(Extension (CubeScan ?source_table_name ?measures ?dimensions ?filters))"
            ),
        ]
    }

    fn is_cube_table(
        &self,
        var: &'static str,
    ) -> impl Fn(&mut EGraph<LogicalPlanLanguage, ()>, Id, &Subst) -> bool {
        let var = var.parse().unwrap();
        let meta_context = self.cube_context.meta.clone();
        move |egraph, _, subst| {
            for node in egraph[subst[var]].nodes.iter() {
                match node {
                    LogicalPlanLanguage::TableScanSourceTableName(TableScanSourceTableName(
                        name,
                    )) => {
                        if meta_context
                            .cubes
                            .iter()
                            .any(|c| c.name.eq_ignore_ascii_case(name))
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        }
    }

    fn has_count_measure(
        &self,
        cube_name: &'static str,
    ) -> impl Fn(&mut EGraph<LogicalPlanLanguage, ()>, Id, &Subst) -> bool {
        let var = cube_name.parse().unwrap();
        let meta_context = self.cube_context.meta.clone();
        move |egraph, _, subst| {
            for node in egraph[subst[var]].nodes.iter() {
                match node {
                    LogicalPlanLanguage::TableScanSourceTableName(TableScanSourceTableName(
                        name,
                    )) => {
                        if let Some(cube) = meta_context
                            .cubes
                            .iter()
                            .find(|c| c.name.eq_ignore_ascii_case(name))
                        {
                            if cube.measures.iter().any(|m| {
                                /*m.name == "count" &&*/
                                m.agg_type == Some("count".to_string())
                            }) {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        }
    }
}

pub struct BestCubePlan;

impl CostFunction<LogicalPlanLanguage> for BestCubePlan {
    type Cost = (/* Cube nodes */ i64, /* AST size */ usize);
    fn cost<C>(&mut self, enode: &LogicalPlanLanguage, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost,
    {
        let this_cube_nodes = match enode {
            LogicalPlanLanguage::CubeScan(_) => -1,
            LogicalPlanLanguage::Measure(_) => -1,
            _ => 0,
        };
        enode
            .children()
            .iter()
            .fold((this_cube_nodes, 1), |(cube_nodes, nodes), id| {
                let (child_cube_nodes, child_nodes) = costs(*id);
                (cube_nodes + child_cube_nodes, nodes + child_nodes)
            })
    }
}

macro_rules! match_params {
    ($converter:expr, $id_expr:expr, $field_variant:ident) => {
        match $id_expr {
            LogicalPlanLanguage::$field_variant(params) => params,
            x => panic!(
                "Expected {} but found {:?}",
                std::stringify!($field_variant),
                x
            ),
        }
    };
}

macro_rules! match_data_node {
    ($converter:expr, $id_expr:expr, $field_variant:ident) => {
        match $converter.best_expr.index($id_expr.clone()) {
            LogicalPlanLanguage::$field_variant($field_variant(data)) => data.clone(),
            x => panic!(
                "Expected {} but found {:?}",
                std::stringify!($field_variant),
                x
            ),
        }
    };
}

macro_rules! match_list_node {
    ($converter:expr, $id_expr:expr, $field_variant:ident) => {{
        fn match_list(
            converter: &LanguageToLogicalPlanConverter<'_>,
            id: Id,
            result: &mut Vec<LogicalPlanLanguage>,
        ) -> Result<(), CubeError> {
            match converter.best_expr.index(id) {
                LogicalPlanLanguage::$field_variant(list) => {
                    for i in list {
                        match_list(converter, *i, result)?;
                    }
                }
                x => {
                    result.push(x.clone());
                }
            }
            Ok(())
        }
        let mut result = Vec::new();
        match_list($converter, $id_expr.clone(), &mut result)?;
        result
    }};
}

macro_rules! match_expr_list_node {
    ($converter:expr, $id_expr:expr, $field_variant:ident) => {{
        fn match_expr_list(
            converter: &LanguageToLogicalPlanConverter<'_>,
            id: Id,
            result: &mut Vec<Expr>,
        ) -> Result<(), CubeError> {
            match converter.best_expr.index(id) {
                LogicalPlanLanguage::$field_variant(list) => {
                    for i in list {
                        match_expr_list(converter, *i, result)?;
                    }
                }
                _ => {
                    result.push(converter.to_expr(id.clone())?);
                }
            }
            Ok(())
        }
        let mut result = Vec::new();
        match_expr_list($converter, $id_expr.clone(), &mut result)?;
        result
    }};
}

pub struct LanguageToLogicalPlanConverter<'a> {
    graph: EGraph<LogicalPlanLanguage, ()>,
    best_expr: RecExpr<LogicalPlanLanguage>,
    cube_context: CubeContext<'a>,
    auth_context: Arc<AuthContext>,
}

impl<'a> LanguageToLogicalPlanConverter<'a> {
    pub fn to_expr(&self, id: Id) -> Result<Expr, CubeError> {
        let node = self.best_expr.index(id);
        Ok(match node {
            LogicalPlanLanguage::AliasExpr(params) => {
                let expr = self.to_expr(params[0].clone())?;
                let alias = match_data_node!(self, params[1], AliasExprAlias);
                Expr::Alias(Box::new(expr), alias)
            }
            LogicalPlanLanguage::ColumnExpr(params) => {
                let column = match_data_node!(self, params[0], ColumnExprColumn);
                Expr::Column(column)
            }
            LogicalPlanLanguage::ScalarVariableExpr(params) => {
                let variable = match_data_node!(self, params[0], ScalarVariableExprVariable);
                Expr::ScalarVariable(variable)
            }
            LogicalPlanLanguage::LiteralExpr(params) => {
                let value = match_data_node!(self, params[0], LiteralExprValue);
                Expr::Literal(value)
            }
            LogicalPlanLanguage::BinaryExpr(params) => {
                let left = Box::new(self.to_expr(params[0].clone())?);
                let op = match_data_node!(self, params[1], BinaryExprOp);
                let right = Box::new(self.to_expr(params[2].clone())?);
                Expr::BinaryExpr { left, op, right }
            }
            LogicalPlanLanguage::NotExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                Expr::Not(expr)
            }
            LogicalPlanLanguage::IsNotNullExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                Expr::IsNotNull(expr)
            }
            LogicalPlanLanguage::IsNullExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                Expr::IsNull(expr)
            }
            LogicalPlanLanguage::NegativeExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                Expr::Negative(expr)
            }
            LogicalPlanLanguage::BetweenExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                let negated = match_data_node!(self, params[1], BetweenExprNegated);
                let low = Box::new(self.to_expr(params[2].clone())?);
                let high = Box::new(self.to_expr(params[3].clone())?);
                Expr::Between {
                    expr,
                    negated,
                    low,
                    high,
                }
            }
            LogicalPlanLanguage::CaseExpr(params) => {
                let expr = match_expr_list_node!(self, params[0], CaseExprExpr);
                let when_then_expr = match_expr_list_node!(self, params[1], CaseExprWhenThenExpr);
                let else_expr = match_expr_list_node!(self, params[2], CaseExprElseExpr);
                Expr::Case {
                    expr: expr.into_iter().next().map(|e| Box::new(e)),
                    when_then_expr: when_then_expr
                        .into_iter()
                        .chunks(2)
                        .into_iter()
                        .map(|mut chunk| {
                            (
                                Box::new(chunk.next().unwrap()),
                                Box::new(chunk.next().unwrap()),
                            )
                        })
                        .collect::<Vec<_>>(),
                    else_expr: else_expr.into_iter().next().map(|e| Box::new(e)),
                }
            }
            LogicalPlanLanguage::CastExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                let data_type = match_data_node!(self, params[1], CastExprDataType);
                Expr::Cast { expr, data_type }
            }
            LogicalPlanLanguage::TryCastExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                let data_type = match_data_node!(self, params[1], TryCastExprDataType);
                Expr::TryCast { expr, data_type }
            }
            LogicalPlanLanguage::SortExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                let asc = match_data_node!(self, params[1], SortExprAsc);
                let nulls_first = match_data_node!(self, params[2], SortExprNullsFirst);
                Expr::Sort {
                    expr,
                    asc,
                    nulls_first,
                }
            }
            LogicalPlanLanguage::ScalarFunctionExpr(params) => {
                let fun = match_data_node!(self, params[0], ScalarFunctionExprFun);
                let args = match_expr_list_node!(self, params[1], ScalarFunctionExprArgs);
                Expr::ScalarFunction { fun, args }
            }
            LogicalPlanLanguage::ScalarUDFExpr(params) => {
                let fun_name = match_data_node!(self, params[0], ScalarUDFExprFun);
                let args = match_expr_list_node!(self, params[1], ScalarUDFExprArgs);
                let fun = self
                    .cube_context
                    .get_function_meta(&fun_name)
                    .ok_or(CubeError::user(format!(
                        "Scalar UDF '{}' is not found",
                        fun_name
                    )))?;
                Expr::ScalarUDF { fun, args }
            }
            LogicalPlanLanguage::AggregateFunctionExpr(params) => {
                let fun = match_data_node!(self, params[0], AggregateFunctionExprFun);
                let args = match_expr_list_node!(self, params[1], AggregateFunctionExprArgs);
                let distinct = match_data_node!(self, params[2], AggregateFunctionExprDistinct);
                Expr::AggregateFunction {
                    fun,
                    args,
                    distinct,
                }
            }
            LogicalPlanLanguage::WindowFunctionExpr(params) => {
                let fun = match_data_node!(self, params[0], WindowFunctionExprFun);
                let args = match_expr_list_node!(self, params[1], WindowFunctionExprArgs);
                let partition_by =
                    match_expr_list_node!(self, params[2], WindowFunctionExprPartitionBy);
                let order_by = match_expr_list_node!(self, params[3], WindowFunctionExprOrderBy);
                let window_frame = match_data_node!(self, params[4], WindowFunctionExprWindowFrame);
                Expr::WindowFunction {
                    fun,
                    args,
                    partition_by,
                    order_by,
                    window_frame,
                }
            }
            LogicalPlanLanguage::AggregateUDFExpr(params) => {
                let fun_name = match_data_node!(self, params[0], AggregateUDFExprFun);
                let args = match_expr_list_node!(self, params[1], AggregateUDFExprArgs);
                let fun =
                    self.cube_context
                        .get_aggregate_meta(&fun_name)
                        .ok_or(CubeError::user(format!(
                            "Aggregate UDF '{}' is not found",
                            fun_name
                        )))?;
                Expr::AggregateUDF { fun, args }
            }
            LogicalPlanLanguage::InListExpr(params) => {
                let expr = Box::new(self.to_expr(params[0].clone())?);
                let list = match_expr_list_node!(self, params[1], InListExprList);
                let negated = match_data_node!(self, params[2], InListExprNegated);
                Expr::InList {
                    expr,
                    list,
                    negated,
                }
            }
            LogicalPlanLanguage::WildcardExpr(_) => Expr::Wildcard,
            x => panic!("Unexpected expression node: {:?}", x),
        })
    }

    pub fn to_logical_plan(&self, id: Id) -> Result<LogicalPlan, CubeError> {
        let node = self.best_expr.index(id);
        Ok(match node {
            LogicalPlanLanguage::Projection(params) => {
                let expr = match_expr_list_node!(self, params[0], ProjectionExpr);
                let input = Arc::new(self.to_logical_plan(params[1])?);
                let alias = match_data_node!(self, params[2], ProjectionAlias);
                let input_schema = DFSchema::new(exprlist_to_fields(&expr, input.schema())?)?;
                let schema = match alias {
                    Some(ref alias) => input_schema.replace_qualifier(alias.as_str()),
                    None => input_schema,
                };
                LogicalPlan::Projection {
                    expr,
                    input,
                    alias,
                    schema: DFSchemaRef::new(schema),
                }
            }
            LogicalPlanLanguage::Filter(params) => {
                let predicate = self.to_expr(params[0])?;
                let input = Arc::new(self.to_logical_plan(params[1])?);
                LogicalPlan::Filter { predicate, input }
            }
            LogicalPlanLanguage::Window(params) => {
                let input = Arc::new(self.to_logical_plan(params[0])?);
                let window_expr = match_expr_list_node!(self, params[1], WindowWindowExpr);
                let mut window_fields: Vec<DFField> =
                    exprlist_to_fields(window_expr.iter(), input.schema())?;
                window_fields.extend_from_slice(input.schema().fields());
                LogicalPlan::Window {
                    input,
                    window_expr,
                    schema: Arc::new(DFSchema::new(window_fields)?),
                }
            }
            LogicalPlanLanguage::Aggregate(params) => {
                let input = Arc::new(self.to_logical_plan(params[0])?);
                let group_expr = match_expr_list_node!(self, params[1], AggregateGroupExpr);
                let aggr_expr = match_expr_list_node!(self, params[2], AggregateAggrExpr);
                let group_expr = normalize_cols(group_expr, &input)?;
                let aggr_expr = normalize_cols(aggr_expr, &input)?;
                let all_expr = group_expr.iter().chain(aggr_expr.iter());
                let schema = Arc::new(DFSchema::new(exprlist_to_fields(
                    all_expr,
                    input.schema(),
                )?)?);
                LogicalPlan::Aggregate {
                    input,
                    group_expr,
                    aggr_expr,
                    schema,
                }
            }
            LogicalPlanLanguage::Sort(params) => {
                let expr = match_expr_list_node!(self, params[0], SortExp);
                let input = Arc::new(self.to_logical_plan(params[1])?);
                LogicalPlan::Sort { expr, input }
            }
            LogicalPlanLanguage::Join(params) => {
                let left = Arc::new(self.to_logical_plan(params[0])?);
                let right = Arc::new(self.to_logical_plan(params[1])?);
                let left_on = match_data_node!(self, params[2], JoinLeftOn);
                let right_on = match_data_node!(self, params[3], JoinRightOn);
                let join_type = match_data_node!(self, params[4], JoinJoinType);
                let join_constraint = match_data_node!(self, params[5], JoinJoinConstraint);
                let schema = Arc::new(build_join_schema(
                    left.schema(),
                    right.schema(),
                    &join_type,
                )?);
                LogicalPlan::Join {
                    left,
                    right,
                    on: left_on.into_iter().zip_eq(right_on.into_iter()).collect(),
                    join_type,
                    join_constraint,
                    schema,
                }
            }
            LogicalPlanLanguage::CrossJoin(params) => {
                let left = Arc::new(self.to_logical_plan(params[0])?);
                let right = Arc::new(self.to_logical_plan(params[1])?);
                let schema = Arc::new(left.schema().join(right.schema())?);
                LogicalPlan::CrossJoin {
                    left,
                    right,
                    schema,
                }
            }
            // // TODO
            // LogicalPlan::Repartition { input, partitioning_scheme: _ } => {
            //     let input = self.add_logical_plan(input.as_ref())?;
            //     self.graph.add(LogicalPlanLanguage::Repartition([input]))
            // }
            // LogicalPlan::Union { inputs, schema: _, alias } => {
            //     let inputs = inputs.iter().map(|e| self.add_logical_plan(e)).collect::<Result<Vec<_>, _>>()?;
            //     let inputs = self.graph.add(LogicalPlanLanguage::UnionInputs(inputs));
            //     let alias = self.graph.add(LogicalPlanLanguage::UnionAlias(UnionAlias(alias.clone())));
            //     self.graph.add(LogicalPlanLanguage::Union([inputs, alias]))
            // }
            LogicalPlanLanguage::TableScan(params) => {
                let source_table_name = match_data_node!(self, params[0], TableScanSourceTableName);
                let table_name = match_data_node!(self, params[1], TableScanTableName);
                let projection = match_data_node!(self, params[2], TableScanProjection);
                let filters = match_expr_list_node!(self, params[3], TableScanFilters);
                let limit = match_data_node!(self, params[4], TableScanLimit);
                let table_parts = source_table_name.split(".").collect::<Vec<_>>();
                let table_reference = if table_parts.len() == 2 {
                    TableReference::Partial {
                        schema: table_parts[0],
                        table: table_parts[1],
                    }
                } else if table_parts.len() == 3 {
                    TableReference::Full {
                        catalog: table_parts[0],
                        schema: table_parts[1],
                        table: table_parts[2],
                    }
                } else {
                    TableReference::from(source_table_name.as_str())
                };
                let provider = self
                    .cube_context
                    .get_table_provider(table_reference)
                    .ok_or(CubeError::user(format!(
                        "Table '{}' is not found",
                        source_table_name
                    )))?;
                let schema = provider.schema();

                let projected_schema = projection
                    .as_ref()
                    .map(|p| {
                        DFSchema::new(
                            p.iter()
                                .map(|i| {
                                    DFField::from_qualified(&table_name, schema.field(*i).clone())
                                })
                                .collect(),
                        )
                    })
                    .unwrap_or_else(|| DFSchema::try_from_qualified_schema(&table_name, &schema))?;
                LogicalPlan::TableScan {
                    table_name,
                    source: provider,
                    projection,
                    projected_schema: Arc::new(projected_schema),
                    filters,
                    limit,
                }
            }
            LogicalPlanLanguage::EmptyRelation(params) => {
                let produce_one_row = match_data_node!(self, params[0], EmptyRelationProduceOneRow);
                LogicalPlan::EmptyRelation {
                    produce_one_row,
                    schema: Arc::new(DFSchema::empty()),
                } // TODO
            }
            LogicalPlanLanguage::Limit(params) => {
                let n = match_data_node!(self, params[0], LimitN);
                let input = Arc::new(self.to_logical_plan(params[1])?);
                LogicalPlan::Limit { n, input }
            }
            // LogicalPlan::CreateExternalTable { .. } => {
            //     panic!("CreateExternalTable is not supported");
            // }
            // LogicalPlan::Values { .. } => {
            //     panic!("Values is not supported");
            // }
            // LogicalPlan::Explain { .. } => {
            //     panic!("Explain is not supported");
            // }
            // LogicalPlan::Analyze { .. } => {
            //     panic!("Analyze is not supported");
            // }
            LogicalPlanLanguage::Extension(params) => {
                let node = match self.best_expr.index(params[0]) {
                    LogicalPlanLanguage::CubeScan(cube_scan_params) => {
                        let cube =
                            match_data_node!(self, cube_scan_params[0], TableScanSourceTableName);
                        let measures =
                            match_list_node!(self, cube_scan_params[1], CubeScanMeasures);
                        let _dimensions =
                            match_list_node!(self, cube_scan_params[2], CubeScanMeasures);
                        // TODO filters
                        // TODO
                        let mut query = V1LoadRequestQuery::new();
                        let mut query_measures = Vec::new();
                        let mut fields = Vec::new();
                        for m in measures {
                            let measure = match_data_node!(
                                self,
                                match_params!(self, m, Measure)[0],
                                MeasureName
                            );
                            query_measures.push(format!("{}.{}", cube, measure));
                            fields.push(DFField::new(
                                None,
                                // TODO alias
                                "COUNT(Uint8(1))",
                                DataType::Int64,
                                true,
                            ));
                        }
                        query.measures = Some(query_measures);
                        query.dimensions = Some(Vec::new());
                        query.segments = Some(Vec::new());
                        Arc::new(CubeScanNode::new(
                            Arc::new(DFSchema::new(fields)?),
                            query,
                            self.auth_context.clone(),
                        ))
                    }
                    x => panic!("Unexpected extension node: {:?}", x),
                };
                LogicalPlan::Extension { node }
            }
            x => panic!("Unexpected logical plan node: {:?}", x),
        })
    }
}
