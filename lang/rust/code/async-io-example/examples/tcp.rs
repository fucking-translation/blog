use mio::Token;
use failure::Error;

const SERVER_ACCEPT: Token = Token(0);
const SERVER: Token = Token(1);
const CLIENT: Token = Token(2);
const SERVER_HELLO: &[u8] = b"PING";
const CLIENT_HELLO: &[u8] = b"PONG";

fn main() -> Result<(), Error> {
    let addr = "127.0.0.1:13265".parse()?;

    Ok(())
}