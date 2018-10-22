macro_rules! jsonrpc_client {
    (
        $(#[$struct_attr:meta])*
        pub struct $struct_name:ident {
            $(
                single:
                $(
                    $(#[$attr_a:meta])*
                    pub fn $method_a:ident(&self$(, $arg_name_a:ident: $arg_ty_a:ty)*) -> Result<$return_ty_a:ty>;
                )*
                enum:
                $(
                    $(#[$attr_b:meta])*
                    pub fn $method_b:ident(&self$(, $arg_name_b:ident: $arg_ty_b:ty)*) -> Result<$($title:ident($return_ty_b:ty))|*>;
                )*
            )+
        }
    ) => {
        use failure::Error;
        use reqwest as rq;
        use serde::Deserialize;
        use std::marker::PhantomData;
        use serde::Serialize;
        use std::sync::{Arc, Condvar, Mutex};
        use uuid::Uuid as req_id;

        #[derive(Deserialize)]
        struct RpcResponse<T> {
            pub result: Option<T>,
            pub error: serde_json::Value,
            pub id: Option<req_id>,
        }

        #[derive(Serialize)]
        struct RpcRequest<T> {
            pub method: String,
            pub params: T,
            pub id: req_id,
        }

        impl<T> RpcRequest<T>
        where T: Serialize {
            pub fn polymorphize(self) -> RpcRequest<serde_json::Value> {
                RpcRequest {
                    method: self.method,
                    params: serde_json::from_str(&serde_json::to_string(&self.params).unwrap()).unwrap(),
                    id: self.id,
                }
            }
        }

        pub mod reply {
            use failure::Error;
            use super::*;
            $(
                $(
                    $(#[$attr_b])*
                    #[derive(Debug)]
                    #[allow(non_camel_case_types)]
                    pub enum $method_b {
                        $($title($return_ty_b),)+
                    }

                    $(#[$attr_b])*
                    impl $method_b {
                        $(
                            #[allow(non_snake_case)]
                            pub fn $title(self) -> Result<$return_ty_b, Error> {
                                match self {
                                    $method_b::$title(a) => Ok(a),
                                    a => bail!("Wrong variant of {}: expected {}(_), got {:?}", stringify!(reply::$method_b), stringify!(reply::$method_b::$return_ty_b), a)
                                }
                            }
                        )+
                    }
               )*
            )+
        }

        pub struct ReqBatcher<T> {
            reqs: Vec<RpcRequest<serde_json::Value>>,
            resps: HashMap<req_id, serde_json::Value>,
            max_batch_size: usize,
            phantom: PhantomData<T>,
        }

        pub trait BatchRequest<$struct_name> {
            fn inner(&self) -> &Mutex<ReqBatcher<$struct_name>>;

            $(
                $(
                    $(#[$attr_a])*
                    fn $method_a(&self$(, $arg_name_a: $arg_ty_a)*) -> Result<req_id, Error>;
                )*
                $(
                    $(#[$attr_b])*
                    fn $method_b(&self$(, $arg_name_b: $arg_ty_b)*) -> Result<req_id, Error>;
                )*
            )*
        }

        impl<'a> BatchRequest<$struct_name> for (&'a $struct_name, &'a Mutex<ReqBatcher<$struct_name>>) {
            fn inner(&self) -> &Mutex<ReqBatcher<$struct_name>> {
                &self.1
            }

            $(
                $(
                    $(#[$attr_a])*
                    fn $method_a(&self$(, $arg_name_a: $arg_ty_a)*) -> Result<req_id, Error> {
                        let id = req_id::new_v4();
                        let body = RpcRequest {
                            method: stringify!($method_a).to_owned(),
                            params: ($($arg_name_a,)*),
                            id,
                        }.polymorphize();
                        let self_lock = self.inner().lock().unwrap();
                        let flush = self_lock.reqs.len() >= self_lock.max_batch_size;
                        drop(self_lock);
                        if flush {
                            let parent = self.0;
                            let batch_response: HashMap<req_id, serde_json::Value> = parent.send_batch()?;
                            let mut self_lock = self.inner().lock().unwrap();
                            self_lock.resps.extend(batch_response);
                            drop(self_lock);
                        }
                        let mut self_lock = self.inner().lock().unwrap();
                        self_lock.reqs.push(body);
                        Ok(id)
                    }
                )*
                $(
                    $(#[$attr_b])*
                    fn $method_b(&self$(, $arg_name_b: $arg_ty_b)*) -> Result<req_id, Error> {
                        println!("queue {}", stringify!($method_b));
                        let id = req_id::new_v4();
                        let body = RpcRequest {
                            method: stringify!($method_b).to_owned(),
                            params: ($($arg_name_b,)*),
                            id,
                        }.polymorphize();
                        println!("{} locking", line!());
                        let self_lock = self.inner().lock().unwrap();
                        println!("{} locked", line!());
                        let flush = self_lock.reqs.len() >= self_lock.max_batch_size;
                        drop(self_lock);
                        if flush {
                            let parent = self.0;
                            let batch_response: HashMap<req_id, serde_json::Value> = parent.send_batch()?;
                            println!("{} locking", line!());
                            let mut self_lock = self.inner().lock().unwrap();
                            println!("{} locked", line!());
                            self_lock.resps.extend(batch_response);
                            drop(self_lock);
                        }
                        let mut self_lock = self.inner().lock().unwrap();
                        self_lock.reqs.push(body);
                        Ok(id)
                    }
                )*
            )*
        }

        $(#[$struct_attr])*
        pub struct $struct_name {
            uri: String,
            user: Option<String>,
            pass: Option<String>,
            max_concurrency: usize,
            rps: usize,
            counter: (Mutex<usize>, Condvar),
            last_req: Mutex<std::time::Instant>,
            batcher: Mutex<ReqBatcher<$struct_name>>,
        }

        impl $struct_name {
            pub fn new(uri: String, user: Option<String>, pass: Option<String>, max_concurrency: usize, rps: usize, max_batch_size: usize) -> Arc<Self> {
                Arc::new($struct_name {
                    uri,
                    user,
                    pass,
                    max_concurrency,
                    rps,
                    counter: (Mutex::new(0), Condvar::new()),
                    last_req: Mutex::new(std::time::Instant::now()),
                    batcher: Mutex::new(ReqBatcher {
                        reqs: Vec::new(),
                        resps: HashMap::new(),
                        max_batch_size,
                        phantom: PhantomData,
                    }),
                })
            }

            pub fn batcher<'a>(&'a self) -> (&'a Self, &'a Mutex<ReqBatcher<$struct_name>>) {
                (self, &self.batcher)
            }

            $(
                $(
                    $(#[$attr_a])*
                    pub fn $method_a(&self$(, $arg_name_a: $arg_ty_a)*) -> Result<$return_ty_a, Error> {
                        println!(stringify!($method_a));
                        let mut builder = rq::Client::new()
                            .post(&self.uri)
                            .header(rq::header::CONNECTION, rq::header::HeaderValue::from_static("close"));
                        match (&self.user, &self.pass) {
                            (Some(ref u), Some(ref p)) => builder = builder.basic_auth(u, Some(p)),
                            (Some(ref u), None) => builder = builder.basic_auth::<&str, &str>(u, None),
                            _ => (),
                        };
                        builder = builder.json(&RpcRequest {
                            method: stringify!($method_a).to_owned(),
                            params: ($($arg_name_a,)*),
                            id: req_id::new_v4(),
                        });
                        if self.rps > 0 {
                            let wait = std::time::Duration::from_secs(1) / self.rps as u32;
                            let mut lock = self.last_req.lock().unwrap();
                            let elapsed = lock.elapsed();
                            if elapsed < wait {
                                std::thread::sleep(wait - elapsed);
                            }
                            *lock = std::time::Instant::now();
                            drop(lock);
                        }
                        if self.max_concurrency > 0 {
                            let mut lock = self.counter.0.lock().unwrap();
                            while *lock == self.max_concurrency {
                                lock = self.counter.1.wait(lock).unwrap();
                            }
                            if *lock > self.max_concurrency {
                                unreachable!();
                            }
                            *lock = *lock + 1;
                            drop(lock);
                        }
                        let mut res = builder.send()?;
                        if self.max_concurrency > 0 {
                            let mut lock = self.counter.0.lock().unwrap();
                            *lock = *lock - 1;
                            self.counter.1.notify_one();
                            drop(lock);
                        }
                        let txt = res.text()?;
                        let body: RpcResponse<$return_ty_a> = serde_json::from_str(&txt)?;
                        match body.result {
                            Some(a) => Ok(a),
                            None => bail!("{:?}", body.error)
                        }
                    }
                )*
                $(
                    $(#[$attr_b])*
                    pub fn $method_b(&self$(, $arg_name_b: $arg_ty_b)*) -> Result<reply::$method_b, Error> {
                        println!(stringify!($method_b));
                        let mut builder = rq::Client::new()
                            .post(&self.uri)
                            .header(rq::header::CONNECTION, rq::header::HeaderValue::from_static("close"));
                        match (&self.user, &self.pass) {
                            (Some(ref u), Some(ref p)) => builder = builder.basic_auth(u, Some(p)),
                            (Some(ref u), None) => builder = builder.basic_auth::<&str, &str>(u, None),
                            _ => (),
                        };
                        builder = builder.json(&RpcRequest {
                            method: stringify!($method_b).to_owned(),
                            params: ($($arg_name_b,)*),
                            id: req_id::new_v4(),
                        });
                        if self.rps > 0 {
                            let wait = std::time::Duration::from_secs(1) / self.rps as u32;
                            let mut lock = self.last_req.lock().unwrap();
                            let elapsed = lock.elapsed();
                            if elapsed < wait {
                                std::thread::sleep(wait - elapsed);
                            }
                            *lock = std::time::Instant::now();
                            drop(lock);
                        }
                        if self.max_concurrency > 0 {
                            let mut lock = self.counter.0.lock().unwrap();
                            while *lock == self.max_concurrency {
                                lock = self.counter.1.wait(lock).unwrap();
                            }
                            if *lock > self.max_concurrency {
                                unreachable!();
                            }
                            *lock = *lock + 1;
                            drop(lock);
                        }
                        let mut res = builder.send()?;
                        if self.max_concurrency > 0 {
                            let mut lock = self.counter.0.lock().unwrap();
                            *lock = *lock - 1;
                            self.counter.1.notify_one();
                            drop(lock);
                        }
                        let txt = res.text()?;
                        let body: reply::$method_b = (|txt: String| {
                            $(
                                match serde_json::from_str::<RpcResponse<$return_ty_b>>(&txt) {
                                    Ok(a) => match a.result {
                                        Some(b) => return Ok(reply::$method_b::$title(b)),
                                        _ => bail!("{:?}", a.error),
                                    },
                                    Err(_) => (),
                                };
                            )+
                            Err(format_err!("Cannot deserialize to any variant of reply::{}", stringify!($method_b)))
                        })(txt)?;
                        Ok(body)
                    }
                )*
            )*
            pub fn send_batch<T: for<'de> Deserialize<'de>>(&self) -> Result<HashMap<req_id, T>, Error> {
                println!("send_batch attempt");
                let mut builder = rq::Client::new()
                    .post(&self.uri)
                    .header(rq::header::CONNECTION, rq::header::HeaderValue::from_static("close"));
                match (&self.user, &self.pass) {
                    (Some(ref u), Some(ref p)) => builder = builder.basic_auth(u, Some(p)),
                    (Some(ref u), None) => builder = builder.basic_auth::<&str, &str>(u, None),
                    _ => (),
                };
                if self.rps > 0 {
                    let wait = std::time::Duration::from_secs(1) / self.rps as u32;
                    let mut lock = self.last_req.lock().unwrap();
                    let elapsed = lock.elapsed();
                    if elapsed < wait {
                        std::thread::sleep(wait - elapsed);
                    }
                    *lock = std::time::Instant::now();
                    drop(lock);
                }
                if self.max_concurrency > 0 {
                    let mut lock = self.counter.0.lock().unwrap();
                    while *lock == self.max_concurrency {
                        lock = self.counter.1.wait(lock).unwrap();
                    }
                    if *lock > self.max_concurrency {
                        unreachable!();
                    }
                    *lock = *lock + 1;
                    drop(lock);
                }
                let mut batcher_lock = self.batcher.lock().unwrap();
                if batcher_lock.reqs.len() == 0 {
                    return Ok(HashMap::new())
                }
                builder = builder.json(&batcher_lock.reqs);
                println!("send_batch {}", batcher_lock.reqs.len());
                let mut res = builder.send()?;
                batcher_lock.reqs = Vec::new();
                let resps = batcher_lock.resps.clone();
                batcher_lock.resps = HashMap::new();
                drop(batcher_lock);
                if self.max_concurrency > 0 {
                    let mut lock = self.counter.0.lock().unwrap();
                    *lock = *lock - 1;
                    self.counter.1.notify_one();
                    drop(lock);
                }
                let text = res.text()?;
                let json = match serde_json::from_str::<Vec<RpcResponse<T>>>(&text) {
                    Ok(a) => a,
                    Err(_) => {
                        bail!("{:?}", serde_json::from_str::<RpcResponse<T>>(&text)?.error)
                    }
                };
                let res_res: Result<HashMap<req_id, T>, Error> = json.into_iter().map(|reply| {
                    Ok(match reply.result {
                        Some(b) => (reply.id.unwrap_or_else(req_id::new_v4), b),
                        _ => bail!("{:?}", reply.error),
                    })
                }).collect();
                let mut res = res_res?;
                for (key, val) in resps {
                    res.insert(key, serde_json::from_str(&serde_json::to_string(&val)?)?);
                }
                Ok(res)
            }
        }
    };
}