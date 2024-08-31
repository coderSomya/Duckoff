use actix::prelude::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct WsConn {
    hb: Instant,
}

impl WsConn {
    fn new() -> Self {
        WsConn { hb: Instant::now() }
    }

    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            if Instant::now().duration_since(act.hb) > Duration::from_secs(10) {
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for WsConn {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsConn {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                ctx.text(text);
            }
            Ok(ws::Message::Binary(bin)) => {
                ctx.binary(bin);
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => (),
        }
    }
}

pub async fn ws_index(
    r: HttpRequest,
    stream: web::Payload,
    srv: web::Data<WsServer>,
) -> Result<HttpResponse, Error> {
    let ws = WsConn::new();

    // Start the WebSocket connection and obtain the actor address
    let resp = ws::start_with_addr(ws, &r, stream);

    match resp {
        Ok((addr, response)) => {
            // Add the session to the server
            srv.add_session(addr);
            Ok(response)
        }
        Err(e) => Err(e),
    }
}

pub struct WsServer {
    sessions: Mutex<Vec<Addr<WsConn>>>,
}

impl WsServer {
    pub fn new() -> Self {
        WsServer {
            sessions: Mutex::new(vec![]),
        }
    }

    pub fn broadcast(&self, message: String) {
        for session in self.sessions.lock().unwrap().iter() {
            session.do_send(WsMessage(message.clone()));
        }
    }

    fn add_session(&self, addr: Addr<WsConn>) {
        self.sessions.lock().unwrap().push(addr);
    }
}

pub struct WsMessage(String);

impl Message for WsMessage {
    type Result = ();
}

impl Handler<WsMessage> for WsConn {
    type Result = ();

    fn handle(&mut self, msg: WsMessage, ctx: &mut Self::Context) {
        ctx.text(msg.0);
    }
}
