use actix_web::dev::{Service, Transform};
use futures::future::{ok, Ready};
use std::task::{Context, Poll};
use tracing_futures::{Instrument, Instrumented};
use uuid::Uuid;

pub(crate) struct Tracing;

pub(crate) struct TracingMiddleware<S> {
    inner: S,
}

impl<S> Transform<S> for Tracing
where
    S: Service,
    S::Future: 'static,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type InitError = ();
    type Transform = TracingMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(TracingMiddleware { inner: service })
    }
}

impl<S> Service for TracingMiddleware<S>
where
    S: Service,
    S::Future: 'static,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type Future = Instrumented<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: S::Request) -> Self::Future {
        let uuid = Uuid::new_v4();

        self.inner
            .call(req)
            .instrument(tracing::info_span!("request", ?uuid))
    }
}
