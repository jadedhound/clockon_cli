#![feature(iter_array_chunks)]

use reqwest::{
    blocking::Client,
    header::{HeaderValue, COOKIE},
};
use std::collections::HashMap;

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
    NoActionToTake,
    BadStatus,
    ResponseUnParsable,
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

#[derive(Debug)]
enum Status {
    ClockedOn,
    ClockedOff,
    OnBreak,
}

impl Status {
    fn to_action(&self, turn_on: bool) -> BoxedError<Action> {
        let action = match self {
            Status::ClockedOn => {
                if turn_on {
                    Action::BreakOn
                } else {
                    Action::ClockOff
                }
            }
            Status::ClockedOff => {
                if turn_on {
                    Action::ClockOn
                } else {
                    Err(MyError::NoActionToTake)?
                }
            }
            Status::OnBreak => {
                if turn_on {
                    Err(MyError::NoActionToTake)?
                } else {
                    Action::BreakOff
                }
            }
        };
        Ok(action)
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Status::ClockedOn => "Clocked On",
            Status::ClockedOff => "Clocked Off",
            Status::OnBreak => "Clocked On (On Break)",
        };
        write!(f, "Status: {s}")
    }
}

#[derive(Debug, PartialEq)]
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

fn get_action_from_result(xml: &str) -> BoxedError<Action> {
    const BRKOFF: &str = "End Break";
    const CLKOFF: &str = "Clock Off";

    let get_inner = |a: &str| {
        a.chars()
            .skip_while(|x| *x != '>')
            .skip(1)
            .take_while(|x| *x != '<')
            .collect::<String>()
    };
    let lines = xml
        .lines()
        .filter(|x| x.contains("caption") || x.contains("enabled") || x.contains("innerhtml"))
        .array_chunks();
    let mut is_on_break = (false, false);
    let mut is_clocking_on = (false, false);
    for [caption, is_enabled] in lines {
        let inner = get_inner(caption);
        if inner == CLKOFF {
            is_clocking_on = (true, get_inner(is_enabled) == "true")
        } else if inner == BRKOFF {
            is_on_break = (true, get_inner(is_enabled) == "true")
        }
    }

    // We need to reverse engineer the action that would lead to
    // this response, hence the flipped action matching
    let action = if is_clocking_on.0 {
        match is_clocking_on.1 {
            true => Action::ClockOn,
            false => Action::ClockOff,
        }
    } else if is_on_break.0 {
        match is_on_break.1 {
            true => Action::BreakOn,
            false => Action::BreakOff,
        }
    } else {
        Err(MyError::ResponseUnParsable)?
    };
    Ok(action)
}

fn do_action(client: &Client, id: &str, action: Action) -> BoxedError<String> {
    println!("Doing action: {action}");
    let url = format!("{URL}$/callback?callback={action}.DoOnAsyncClick&which=0&modifiers=");
    let res = client.post(url).header(COOKIE, id).send()?;
    let body = res.text()?;
    if action == get_action_from_result(&body)? {
        Ok(body)
    } else {
        Err(MyError::ActionFailure(body.len()))?
    }
}

fn login(client: &Client, id: &str) -> BoxedError<String> {
    println!("Logging in");
    let form = HashMap::from([
        ("USRNMEEDT", "var"),
        ("PSSWRDEDT", "var"),
        ("IW_Action", "LOGINBTN"),
    ]);
    let res = client.post(URL).header(COOKIE, id).form(&form).send()?;
    let body = res.text()?;
    if body.len() < 75000 {
        Ok(body)
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

fn get_status(html: &str) -> BoxedError<Status> {
    let get_inner = |x: &str| {
        x.chars()
            .skip(4)
            .take_while(|a| *a != '"')
            .collect::<String>()
    };
    let mut lines = html.lines().filter(|x| x.contains("DISABLED"));
    let mut get_id = || {
        lines
            .next()
            .and_then(|x| x.split_whitespace().find(|a| a.contains("ID=")))
            .map(get_inner)
            .ok_or(MyError::BadStatus)
    };
    const BRKOFF: &str = "BRKENDBTN";
    const BRKON: &str = "BRKSTABTN";
    let id_one = get_id()?;
    let id_two = get_id()?;
    // If there are 3 disabled btns then we are logged off
    let status = if lines.next().is_some() {
        Status::ClockedOff
    } else if id_one == BRKON || id_two == BRKON {
        Status::OnBreak
    } else if id_one == BRKOFF || id_two == BRKOFF {
        Status::ClockedOn
    } else {
        Err(MyError::BadStatus)?
    };
    println!("{status}");
    Ok(status)
}

fn get_operator() -> BoxedError<bool> {
    let op = std::env::args().nth(1).ok_or(MyError::NoOperator)?;
    if op == "on" {
        Ok(true)
    } else if op == "off" {
        Ok(false)
    } else {
        Err(MyError::NoOperator)?
    }
}

fn main() -> BoxedError<()> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(UAGENT)
        .danger_accept_invalid_certs(true)
        .build()?;
    let id = extract_session_id(get_cookie(&client)?)?;
    let body = login(&client, &id)?;
    let status = get_status(&body)?;
    let action = status.to_action(get_operator()?)?;
    let _body = do_action(&client, &id, action)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_action_from_xml(xml: &str) -> Action {
        let body = std::fs::read_to_string(format!("./examples/{xml}.xml")).expect("xml exists");
        get_action_from_result(&body).expect("parse action from xml page")
    }

    #[test]
    fn read_action_from_response() {
        assert_eq!(Action::ClockOff, get_action_from_xml("clock_off"));
        assert_eq!(Action::ClockOn, get_action_from_xml("clock_on"));
        assert_eq!(Action::BreakOn, get_action_from_xml("break_on"));
        assert_eq!(Action::BreakOff, get_action_from_xml("break_off"));
    }

    fn action_from_page(page: &str, to_action: bool) -> Action {
        let body = std::fs::read_to_string(format!("./examples/{page}.html"))
            .expect("html example exists");
        let result = get_status(&body).expect("able to get status");
        result.to_action(to_action).expect("get action from result")
    }

    fn err_from_page(page: &str, to_action: bool) -> bool {
        let body = std::fs::read_to_string(format!("./examples/{page}.html"))
            .expect("html example exists");
        let result = get_status(&body).expect("able to get status");
        let err = result
            .to_action(to_action)
            .expect_err("get action from result");
        format!("{:?}", MyError::NoActionToTake) == format!("{:?}", err)
    }

    #[test]
    fn decide_action_from_login_page() {
        assert_eq!(Action::BreakOn, action_from_page("clockon", true));
        assert_eq!(Action::ClockOff, action_from_page("clockon", false));
        assert_eq!(Action::ClockOn, action_from_page("clockoff", true));
        assert!(err_from_page("clockoff", false));
        assert!(err_from_page("break_on", true));
        assert_eq!(Action::BreakOff, action_from_page("break_on", false));
    }
}
