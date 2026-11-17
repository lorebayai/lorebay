use typed_builder::TypedBuilder;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Caller {
    pub account_id: String,
    pub login_session_id: String,
}
