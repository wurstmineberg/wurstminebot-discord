use {
    pyo3::{
        create_exception,
        prelude::*,
        wrap_pyfunction,
    },
    serenity::{
        model::prelude::*,
        utils::MessageBuilder,
    },
};

create_exception!(wurstminebot, CommandError, pyo3::exceptions::PyRuntimeError);

fn user_to_id(user: &Bound<'_, PyAny>) -> PyResult<UserId> {
    if let Ok(snowflake) = user.getattr("snowflake") {
        // support wurstmineberg_web.models.Person arguments
        Ok(UserId::new(snowflake.extract()?))
    } else if let Ok(wmbid) = user.getattr("wmbid") {
        Err(CommandError::new_err(format!("Wurstmineberg member {} has no Discord snowflake", wmbid)))
    } else {
        // support plain snowflakes
        Ok(UserId::new(user.extract()?))
    }
}

#[pyfunction] fn escape(text: &str) -> String {
    let mut builder = MessageBuilder::default();
    builder.push_safe(text);
    builder.build()
}

#[pyfunction] fn channel_msg(channel_id: u64, msg: String) -> PyResult<()> {
    wurstminebot_ipc::channel_msg(ChannelId::new(channel_id), msg)
        .map_err(|e| CommandError::new_err(e.to_string()))
}

#[pyfunction] fn quit() -> PyResult<()> {
    wurstminebot_ipc::quit()
        .map_err(|e| CommandError::new_err(e.to_string()))
}

#[pyfunction] fn set_display_name(user_id: &Bound<'_, PyAny>, new_display_name: String) -> PyResult<()> {
    wurstminebot_ipc::set_display_name(user_to_id(user_id)?, new_display_name)
        .map_err(|e| CommandError::new_err(e.to_string()))
}

#[pymodule] fn wurstminebot(_: Python<'_>, m: Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(escape, m.clone())?)?;
    //TODO make sure that all IPC commands are listed below
    m.add_function(wrap_pyfunction!(channel_msg, m.clone())?)?;
    m.add_function(wrap_pyfunction!(quit, m.clone())?)?;
    m.add_function(wrap_pyfunction!(set_display_name, m.clone())?)?;
    Ok(())
}
