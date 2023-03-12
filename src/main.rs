use std::collections::HashMap;

use reqwest::{
    blocking::Client,
    header::{HeaderValue, COOKIE},
};

type BoxedError<T> = Result<T, Box<dyn std::error::Error>>;

const URL: &str = "https://webportal.clockon.com.au:4465/";
const UAGENT: &str = "Mozilla/5.0 (Windows NT 10.0; rv:110.0) Gecko/20100101 Firefox/110.0";

#[derive(Debug)]
enum MyError {
    BadHeaderLen(String),
    NoHeader,
    LoginFailure,
    ActionFailure(usize),
    NoOperator,
    NoOperatorMod,
}

impl std::error::Error for MyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.source()
    }
}

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Custom Error")
    }
}

enum Action {
    ClockOn,
    ClockOff,
    BreakOn,
    BreakOff,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::ClockOn => write!(f, "CLKONBTN"),
            Action::ClockOff => write!(f, "CLKOFFBTN"),
            Action::BreakOn => write!(f, "BRKSTABTN"),
            Action::BreakOff => write!(f, "BRKENDBTN"),
        }
    }
}

fn extract_session_id(header: HeaderValue) -> BoxedError<String> {
    println!("Extracting session ID");
    let id: String = header.to_str()?.chars().take(45).collect();
    if id.len() == 45 {
        Ok(id)
    } else {
        Err(MyError::BadHeaderLen(id))?
    }
}

fn do_action(client: &Client, id: &str, action: Action) -> BoxedError<()> {
    println!("Doing action: {action}");
    let url = format!("{URL}$/callback?callback={action}.DoOnAsyncClick&which=0&modifiers=");
    let res = client.post(url).header(COOKIE, id).send()?;
    let body = res.text()?;
    if body.len() > 500 {
        Ok(())
    } else {
        Err(MyError::ActionFailure(body.len()))?
    }
}

fn login(client: &Client, id: &str) -> BoxedError<()> {
    println!("Logging in");
    let form = HashMap::from([
        ("USRNMEEDT", "var"),
        ("PSSWRDEDT", "var"),
        ("IW_Action", "LOGINBTN"),
    ]);
    let res = client.post(URL).header(COOKIE, id).form(&form).send()?;
    let body = res.text()?;
    if body.len() < 75000 {
        Ok(())
    } else {
        Err(MyError::LoginFailure)?
    }
}

fn get_cookie(client: &Client) -> BoxedError<HeaderValue> {
    println!("Requesting cookie");
    let resp = client.get(URL).send()?;
    let cookie = resp
        .headers()
        .get("Set-Cookie")
        .cloned()
        .ok_or(MyError::NoHeader)?;
    Ok(cookie)
}

fn get_action() -> BoxedError<Action> {
    let mut args = std::env::args();
    let operator = args.nth(1).ok_or(MyError::NoOperator)?;
    let modifier = args.next().ok_or(MyError::NoOperatorMod)?;
    let action = match operator.as_ref() {
        "log" => match modifier.as_ref() {
            "on" => Action::ClockOn,
            _ => Action::ClockOff,
        },
        "break" => match modifier.as_ref() {
            "on" => Action::BreakOn,
            _ => Action::BreakOff,
        },
        _ => Err(MyError::NoOperator)?,
    };
    Ok(action)
}

fn main() -> BoxedError<()> {
    let action = get_action()?;
    let client = reqwest::blocking::Client::builder()
        .user_agent(UAGENT)
        .danger_accept_invalid_certs(true)
        .build()?;
    let id = extract_session_id(get_cookie(&client)?)?;
    login(&client, &id)?;
    do_action(&client, &id, action)?;
    Ok(())
}
