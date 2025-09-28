use actix_web::{
    Error, HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use futures_util::future::{LocalBoxFuture, Ready, ready};
use std::rc::Rc;
use uuid::Uuid;

/// Middleware добавляющий trace/request id
pub struct RequestId;

impl<S, B> Transform<S, ServiceRequest> for RequestId
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RequestIdMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestIdMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct RequestIdMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for RequestIdMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &self,
        ctx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let id = Uuid::new_v4().to_string();
        req.extensions_mut().insert(id.clone());
        log::debug!("request_id={} path={}", id, req.path());
        let fut = self.service.call(req);
        Box::pin(async move {
            let resp = fut.await?;
            Ok(resp)
        })
    }
}
