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

#[derive(Debug)]
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

fn do_action(client: &Client, id: &str, action: Action) -> BoxedError<String> {
    println!("Doing action: {action}");
    let url = format!("{URL}$/callback?callback={action}.DoOnAsyncClick&which=0&modifiers=");
    let res = client.post(url).header(COOKIE, id).send()?;
    let body = res.text()?;
    if body.len() > 500 {
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

fn get_disabled_status(line: &str) -> bool {
    let substr: String = line.chars().skip(30).take(8).collect();
    substr == "DISABLED"
}

fn get_status(html: &str) -> BoxedError<Status> {
    let mut lines = html.lines().into_iter().filter(|x| x.contains("DISABLED"));
    let mut get_id = || {
        lines
            .next()
            .and_then(|x| x.split_whitespace().find(|a| a.contains("ID=")))
            .map(|x| x.chars().skip(3).collect::<String>())
            .ok_or(MyError::BadStatus)
    };
    const BRKOFF: &str = "\"BRKENDBTN\"";
    const BRKON: &str = "\"BRKSTABTN\"";
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

    #[test]
    fn action_if_clocked_on_and_on() {
        let body =
            std::fs::read_to_string("./examples/clockon.html").expect("unable to find test file");
        let result = get_status(&body).expect("should be able to get status");
        let action = result.to_action(true).expect("should get action");
        assert_eq!("BRKSTABTN", action.to_string());
    }
    #[test]
    fn action_if_clocked_on_and_off() {
        let body =
            std::fs::read_to_string("./examples/clockon.html").expect("unable to find test file");
        let result = get_status(&body).expect("should be able to get status");
        let action = result.to_action(false).expect("should get action");
        assert_eq!("CLKOFFBTN", action.to_string());
    }
    #[test]
    fn action_if_clocked_off_and_on() {
        let body =
            std::fs::read_to_string("./examples/clockoff.html").expect("unable to find test file");
        let result = get_status(&body).expect("should be able to get status");
        let action = result.to_action(true).expect("should get action");
        assert_eq!("CLKONBTN", action.to_string());
    }
    #[test]
    fn action_if_clocked_off_and_off() {
        let body =
            std::fs::read_to_string("./examples/clockoff.html").expect("unable to find test file");
        let result = get_status(&body).expect("should be able to get status");
        let action = result.to_action(false).expect_err("should be an error");
        assert_eq!(
            format!("{:?}", MyError::NoActionToTake),
            format!("{:?}", action)
        );
    }
    #[test]
    fn action_if_on_break_and_on() {
        let body =
            std::fs::read_to_string("./examples/break_on.html").expect("unable to find test file");
        let result = get_status(&body).expect("should be able to get status");
        let action = result.to_action(true).expect_err("should be an error");
        assert_eq!(
            format!("{:?}", MyError::NoActionToTake),
            format!("{:?}", action)
        );
    }
    #[test]
    fn action_if_on_break_and_off() {
        let body =
            std::fs::read_to_string("./examples/break_on.html").expect("unable to find test file");
        let result = get_status(&body).expect("should be able to get status");
        let action = result.to_action(false).expect("should get action");
        assert_eq!("BRKENDBTN", action.to_string());
    }
}
