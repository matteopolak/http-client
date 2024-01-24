use std::io::{self, Read, Write};
use std::net::TcpStream;

use serde::Serialize;
use url::{ParseError, Url};

use super::header::Header;
use super::response::Response;

#[derive(Debug)]
pub enum Method {
	Get,
	Post,
}

impl Method {
	pub fn as_bytes(&self) -> &[u8] {
		match self {
			Self::Get => b"GET",
			Self::Post => b"POST",
		}
	}
}

#[derive(Debug)]
pub struct Request {
	url: Url,
	method: Method,
	body: Option<Vec<u8>>,
	headers: Vec<Header>,
}

impl Request {
	pub const BUF_SIZE: usize = 1024;

	pub fn get<U: TryInto<Url, Error = ParseError>>(url: U) -> RequestBuilder {
		RequestBuilder::new(Method::Get, url)
	}

	pub fn post<U: TryInto<Url, Error = ParseError>>(url: U) -> RequestBuilder {
		RequestBuilder::new(Method::Post, url)
	}

	pub fn send(self) -> Result<Response, io::Error> {
		let mut stream = TcpStream::connect(self.url.socket_addrs(|| None)?.as_slice())?;

		self.write(&mut stream)?;
		stream.flush()?;

		let mut sink = Vec::new();
		let mut buf = [0u8; Self::BUF_SIZE];

		loop {
			let n = stream.read(&mut buf)?;

			sink.extend_from_slice(&buf[..n]);

			if n < Self::BUF_SIZE {
				break;
			}
		}

		Response::from_bytes(sink)
	}

	fn write(&self, stream: &mut TcpStream) -> io::Result<()> {
		stream.write_all(self.method.as_bytes())?;
		stream.write_all(b" ")?;
		stream.write_all(self.url.path().as_bytes())?;

		if let Some(query) = self.url.query() {
			stream.write_all(query.as_bytes())?;
		}

		stream.write_all(b" HTTP/1.1\r\n")?;

		for header in &self.headers {
			stream.write_all(header.name.as_bytes())?;
			stream.write_all(b": ")?;
			stream.write_all(header.value.as_bytes())?;
			stream.write_all(b"\r\n")?;
		}

		if let Some(body) = &self.body {
			stream.write_all(b"\r\n")?;
			stream.write_all(body.as_slice())?;
		}

		Ok(())
	}
}

pub struct RequestBuilder {
	request: Request,
}

impl RequestBuilder {
	pub fn new<U: TryInto<Url, Error = ParseError>>(method: Method, url: U) -> Self {
		let url = url.try_into().unwrap();

		Self {
			request: Request {
				method,
				body: None,
				headers: if let Some(host) = url.host_str() {
					vec![Header {
						name: "host".into(),
						value: host.into(),
					}]
				} else {
					vec![]
				},
				url,
			},
		}
	}

	pub fn send(self) -> Result<Response, io::Error> {
		self.request.send()
	}

	pub fn header<N: Into<String>, V: Into<String>>(mut self, name: N, value: V) -> Self {
		self.request.headers.push(Header {
			name: name.into(),
			value: value.into(),
		});
		self
	}

	pub fn json<T: Serialize + ?Sized>(mut self, payload: &T) -> Self {
		// FIXME: handle errors
		let bytes = serde_json::to_vec(payload).expect("invalid JSON body");
		let len = bytes.len();

		self.request.body = Some(bytes);
		self.header("content-length", format!("{len}"))
			.header("content-type", "application/json")
	}
}
